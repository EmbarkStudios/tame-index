//! Provides functionality for reading and writing cargo compatible .cache entries
//! that can be wrapped by another index that has logic for fetching entries
//! that aren't in the cache
//!
//! Cargo creates small cache entries for crates when they are accessed during
//! any cargo operation that accesses a registry index (update/add/etc).
//! Initially this was to accelerate accessing the contents of a bare clone of
//! a git registry index as it skips accessing git blobs.
//!
//! Now with sparse HTTP indices, these .cache files are even more important as
//! they allow skipping network access if in offline mode, as well as allowing
//! responses from servers to tell the client they have the latest version if
//! that crate has not been changed since it was last accessed.
//!
//! ```txt
//! +-------------------+---------------------------+------------------+---+
//! | cache version :u8 | index format version :u32 | revision :string | 0 |
//! +-------------------+---------------------------+------------------+---+
//! ```
//!
//! followed by 1+
//!
//! ```txt
//! +----------------+---+-----------+---+
//! | semver version | 0 | JSON blob | 0 |
//! +----------------+---+-----------+---+
//! ```

/// The current (cargo 1.54.0+) cache version for cache entries.
///
/// This value's sole purpose is in determining if cargo will read or skip (and
/// probably overwrite) a .cache entry.
pub const CURRENT_CACHE_VERSION: u8 = 3;
/// The maximum version of the `v` field in the index this crate supports
pub const INDEX_V_MAX: u32 = 2;
/// The byte representation of [`INDEX_V_MAX`]
const INDEX_V_MAX_BYTES: [u8; 4] = INDEX_V_MAX.to_le_bytes();

use super::FileLock;
use crate::{CacheError, Error, IndexKrate, KrateName, PathBuf};

/// A wrapper around a byte buffer that has been (partially) validated to be a
/// valid cache entry
pub struct ValidCacheEntry<'buffer> {
    /// The cache entry's revision
    ///
    /// For git indices this will be the sha1 of the HEAD commit when the cache
    /// entry was written
    ///
    /// For sparse indices, this will be an HTTP header from the response that
    /// was last written to disk, which is currently either `etag: <etag>` or
    /// `last-modified: <timestamp>`, or `Unknown` if an HTTP response was otherwise
    /// valid but contained neither of those headers
    pub revision: Revision<'buffer>,
    /// Portion of the buffer containing the individual version entries for the
    /// cache entry
    pub version_entries: &'buffer [u8],
}

impl<'buffer> ValidCacheEntry<'buffer> {
    /// Attempts to read a cache entry from a block of bytes.
    ///
    /// This can fail for a few reasons
    /// 1. The cache version does not match the version(s) supported
    /// 2. The index version is higher than that supported
    /// 3. There is not at least 1 version entry
    pub fn read(mut buffer: &'buffer [u8]) -> Result<Self, CacheError> {
        let cache_version = *buffer.first().ok_or(CacheError::InvalidCacheEntry)?;

        match cache_version.cmp(&CURRENT_CACHE_VERSION) {
            std::cmp::Ordering::Less => return Err(CacheError::OutdatedCacheVersion),
            std::cmp::Ordering::Greater => return Err(CacheError::UnknownCacheVersion),
            std::cmp::Ordering::Equal => {}
        }

        buffer = &buffer[1..];
        let index_version = u32::from_le_bytes(
            buffer
                .get(0..4)
                .ok_or(CacheError::InvalidCacheEntry)
                .and_then(|b| b.try_into().map_err(|_e| CacheError::InvalidCacheEntry))?,
        );

        if INDEX_V_MAX > index_version {
            return Err(CacheError::UnknownIndexVersion);
        }

        buffer = &buffer[4..];

        let mut iter = split(buffer, 0);
        let revision = std::str::from_utf8(iter.next().ok_or(CacheError::InvalidCacheEntry)?)
            .map_err(|_e| CacheError::OutdatedRevision)?;

        // Ensure there is at least one valid entry, it _should_ be impossible
        // to have an empty cache entry since you can't publish something to an
        // index and still have zero versions
        let _version = iter.next().ok_or(CacheError::InvalidCacheEntry)?;
        let _blob = iter.next().ok_or(CacheError::InvalidCacheEntry)?;

        let version_entries = &buffer[revision.len() + 1..];

        let revision = if let Some((name, value)) = revision.split_once(": ") {
            match name {
                "etag" => Revision::Http(HttpRevision::Etag(value)),
                "last-modified" => Revision::Http(HttpRevision::LastModified(value)),
                _ => return Err(CacheError::InvalidCacheEntry),
            }
        } else {
            if revision.len() == 40 && revision.chars().all(|c: char| c.is_ascii_hexdigit()) {
                let mut r = [0u8; 40];
                r.copy_from_slice(revision.as_bytes());
                Revision::Git(GitRevision::Hex(crate::index::git::Sha1Hex::internal(r)))
            } else if revision == UNKNOWN {
                Revision::Unknown
            } else {
                return Err(CacheError::InvalidCacheEntry);
            }
        };

        Ok(Self {
            revision,
            version_entries,
        })
    }

    /// Deserializes this cache entry into a [`IndexKrate`]
    ///
    /// If specified, the `revision` will be used to ignore cache entries
    /// that are outdated
    pub fn to_krate(&self, revision: Option<Revision<'_>>) -> Result<Option<IndexKrate>, Error> {
        if let Some(revision) = revision {
            if revision != self.revision {
                return Ok(None);
            }
        }

        Ok(Some(IndexKrate::from_cache(split(
            self.version_entries,
            0,
        ))?))
    }
}

impl IndexKrate {
    /// Reads entries from the versions portion of a cache file
    pub(crate) fn from_cache<'cache>(
        mut iter: impl Iterator<Item = &'cache [u8]> + 'cache,
    ) -> Result<Self, Error> {
        let mut versions = Vec::new();

        // Each entry is a tuple of (semver, version_json)
        while iter.next().is_some() {
            let version_slice = iter
                .next()
                .ok_or(Error::Cache(CacheError::InvalidCrateVersion))?;
            let version: crate::IndexVersion = serde_json::from_slice(version_slice)?;
            versions.push(version);
        }

        Ok(Self { versions })
    }

    /// Writes a cache entry with the specified revision to an [`std::io::Write`]
    ///
    /// Note this method creates its own internal [`std::io::BufWriter`], there
    /// is no need to wrap it yourself
    pub fn write_cache_entry<W: std::io::Write>(
        &self,
        writer: &mut W,
        revision: &Revision<'_>,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;

        const SPLIT: &[u8] = &[0];

        const PAGE_SIZE: usize = 4 * 1024;

        /// std::io::BufWriter does a heap allocation for its scratch buffer, so just avoid that in favor
        /// of a small stack buffer instead
        struct StackWriter<'w, W> {
            buf: [u8; PAGE_SIZE],
            index: usize,
            inner: &'w mut W,
        }

        impl<'w, W> StackWriter<'w, W>
        where
            W: std::io::Write,
        {
            fn new(inner: &'w mut W) -> Self {
                Self {
                    buf: [0u8; PAGE_SIZE],
                    index: 0,
                    inner,
                }
            }
        }

        impl<'w, W> std::io::Write for StackWriter<'w, W>
        where
            W: std::io::Write,
        {
            fn write(&mut self, mut buf: &[u8]) -> std::io::Result<usize> {
                let len = buf.len();
                let spare = PAGE_SIZE - self.index;
                if len > spare {
                    self.buf[self.index..].copy_from_slice(&buf[..spare]);
                    buf = &buf[spare..];
                    self.inner.write_all(&self.buf[..])?;
                    self.index = 0;
                }

                if buf.len() >= PAGE_SIZE {
                    let mut iter = buf.chunks_exact(PAGE_SIZE);

                    for chunk in &mut iter {
                        self.inner.write_all(chunk)?;
                    }

                    buf = iter.remainder();
                }

                if !buf.is_empty() {
                    self.buf[self.index..self.index + buf.len()].copy_from_slice(buf);
                    self.index += buf.len();
                }

                Ok(len)
            }

            fn flush(&mut self) -> std::io::Result<()> {
                if self.index > 0 {
                    self.inner.write_all(&self.buf[..self.index])?;
                    self.index = 0;
                }

                Ok(())
            }
        }

        let mut w = StackWriter::new(writer);
        w.write_all(&[CURRENT_CACHE_VERSION])?;
        w.write_all(&INDEX_V_MAX_BYTES)?;
        revision.write(&mut w)?;
        w.write_all(SPLIT)?;

        // crates.io limits crate names to a maximum of 64 characters, but this
        // only applies to crates.io and not any cargo index, so don't set a hard
        // limit
        let mut semver = String::with_capacity(64);

        for iv in &self.versions {
            semver.clear();
            // SAFETY: the only way this would fail would be OOM
            std::fmt::write(&mut semver, format_args!("{}", iv.version)).unwrap();
            w.write_all(semver.as_bytes())?;
            w.write_all(SPLIT)?;

            serde_json::to_writer(&mut w, &iv)?;
            w.write_all(SPLIT)?;
        }

        w.flush()
    }
}

/// Gives an iterator over the specified buffer, where each item is split by the specified
/// needle value
pub fn split(haystack: &[u8], needle: u8) -> impl Iterator<Item = &[u8]> + '_ {
    struct Split<'a> {
        haystack: &'a [u8],
        needle: u8,
    }

    impl<'a> Iterator for Split<'a> {
        type Item = &'a [u8];

        #[inline]
        fn next(&mut self) -> Option<&'a [u8]> {
            if self.haystack.is_empty() {
                return None;
            }
            let (ret, remaining) = match memchr::memchr(self.needle, self.haystack) {
                Some(pos) => (&self.haystack[..pos], &self.haystack[pos + 1..]),
                None => (self.haystack, &[][..]),
            };
            self.haystack = remaining;
            Some(ret)
        }
    }

    Split { haystack, needle }
}

const ETAG: &str = "etag: ";
const LAST_MODIFIED: &str = "last-modified: ";
const UNKNOWN: &str = "Unknown";

pub use crate::index::{git::GitRevision, sparse::HttpRevision};

/// A cache entry revision
pub enum Revision<'stack> {
    /// An HTTP revision from either the `etag` or `last-modified` header value
    Http(HttpRevision<'stack>),
    /// A git revision, either a raw sha1 or a hex-encoded sha1 of the `HEAD` commit of the git registry
    Git(GitRevision),
    /// Unknown revision
    Unknown,
}

impl Revision<'_> {
    /// Writes this revision to the specified writer, in the same scheme as cargo
    #[inline]
    pub fn write(&self, w: &mut impl std::io::Write) -> std::io::Result<usize> {
        let written = match self {
            Self::Http(http) => match http {
                HttpRevision::Etag(etag) => {
                    w.write_all(ETAG.as_bytes())?;
                    w.write_all(etag.as_bytes())?;
                    ETAG.len() + etag.len()
                }
                HttpRevision::LastModified(lm) => {
                    w.write_all(LAST_MODIFIED.as_bytes())?;
                    w.write_all(lm.as_bytes())?;
                    LAST_MODIFIED.len() + lm.len()
                }
            },
            Self::Git(git) => match git {
                GitRevision::Raw(sha1) => {
                    let mut out = [0u8; 40];
                    crate::utils::encode_hex(sha1, &mut out);
                    w.write_all(&out)?;
                    out.len()
                }
                GitRevision::Hex(sha1) => {
                    w.write_all(sha1.as_ref())?;
                    sha1.len()
                }
            },
            Self::Unknown => {
                w.write_all(UNKNOWN.as_bytes())?;
                UNKNOWN.len()
            }
        };

        Ok(written)
    }
}

impl PartialEq for Revision<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Http(a), Self::Http(b)) => match (a, b) {
                (HttpRevision::Etag(a), HttpRevision::Etag(b)) => a == b,
                (HttpRevision::LastModified(a), HttpRevision::LastModified(b)) => a == b,
                _ => false,
            },
            (Self::Git(a), Self::Git(b)) => a == b,
            (Self::Unknown, Self::Unknown) => true,
            _ => false,
        }
    }
}

use std::fmt;

impl fmt::Debug for Revision<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(http) => {
                let (n, v) = match http {
                    HttpRevision::Etag(etag) => (ETAG, etag),
                    HttpRevision::LastModified(lm) => (LAST_MODIFIED, lm),
                };

                f.write_str(n)?;
                f.write_str(": ")?;
                f.write_str(v)
            }
            Self::Git(git) => match git {
                GitRevision::Hex(hex) => f.write_str(hex.as_ref()),
                GitRevision::Raw(raw) => {
                    let mut output = [0u8; 40];
                    f.write_str(crate::utils::encode_hex(raw, &mut output))
                }
            },
            Self::Unknown => f.write_str("Unknown"),
        }
    }
}

/// The [`IndexCache`] allows access to the local cache entries for a remote index
///
/// This implementation does no network I/O whatsoever, but does do disk I/O
pub struct IndexCache {
    /// The root disk location of the local index
    pub(super) path: PathBuf,
}

impl IndexCache {
    /// Creates a local index exactly at the specified path
    #[inline]
    pub fn at_path(path: PathBuf) -> Self {
        Self { path }
    }

    /// Reads a crate from the local cache of the index.
    ///
    /// You may optionally pass in the revision the cache entry is expected to
    /// have, if it does match the cache entry will be ignored and an error returned
    #[inline]
    pub fn cached_krate(
        &self,
        name: KrateName<'_>,
        revision: Option<Revision<'_>>,
        lock: &FileLock,
    ) -> Result<Option<IndexKrate>, Error> {
        let Some(contents) = self.read_cache_file(name, lock)? else {
            return Ok(None);
        };

        let valid = ValidCacheEntry::read(&contents)?;
        valid.to_krate(revision)
    }

    /// Writes the specified crate and revision to the cache
    pub fn write_to_cache(
        &self,
        krate: &IndexKrate,
        revision: &Revision<'_>,
        _lock: &FileLock,
    ) -> Result<PathBuf, Error> {
        let name = krate.name().try_into()?;
        let cache_path = self.cache_path(name);

        std::fs::create_dir_all(cache_path.parent().unwrap())?;

        let mut cache_file = match std::fs::File::create(&cache_path) {
            Ok(cf) => cf,
            Err(err) => return Err(Error::IoPath(err, cache_path)),
        };

        // It's unfortunate if this fails for some reason, but
        // not writing the cache entry shouldn't stop the user
        // from getting the crate's metadata
        match krate.write_cache_entry(&mut cache_file, revision) {
            Ok(_) => Ok(cache_path),
            Err(err) => {
                drop(cache_file);
                // _attempt_ to delete the file, to clean up after ourselves
                let _ = std::fs::remove_file(&cache_path);
                Err(Error::IoPath(err, cache_path))
            }
        }
    }

    /// Gets the path the crate's cache file would be located at if it exists
    #[inline]
    pub fn cache_path(&self, name: KrateName<'_>) -> PathBuf {
        let rel_path = name.relative_path(None);

        // avoid realloc on each push
        let mut cache_path = PathBuf::with_capacity(self.path.as_str().len() + 8 + rel_path.len());
        cache_path.push(&self.path);
        cache_path.push(".cache");
        cache_path.push(rel_path);

        cache_path
    }

    /// Attempts to read the cache entry for the specified crate
    ///
    /// It is recommended to use [`Self::cached_krate`]
    #[inline]
    pub fn read_cache_file(
        &self,
        name: KrateName<'_>,
        _lock: &FileLock,
    ) -> Result<Option<Vec<u8>>, Error> {
        let cache_path = self.cache_path(name);

        match std::fs::read(&cache_path) {
            Ok(cb) => Ok(Some(cb)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(Error::IoPath(err, cache_path)),
        }
    }
}
