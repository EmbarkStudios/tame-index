//! Provides functionality for reading and writing cargo compatible .cache entries
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
//!
//!
//! +-------------------+---------------------------+------------------+---+
//! | cache version :u8 | index format version :u32 | revision :string | 0 |
//! +-------------------+---------------------------+------------------+---+
//!
//! followed by 1+
//!
//! +----------------+---+-----------+---+
//! | semver version | 0 | JSON blob | 0 |
//! +----------------+---+-----------+---+

/// The current (cargo 1.54.0+) cache version for cache entries.
///
/// This value's sole purpose is in determining if cargo will read or skip (and
/// probably overwrite) a .cache entry.
pub const CURRENT_CACHE_VERSION: u8 = 3;
/// The maximum version of the `v` field in the index this crate supports
pub const INDEX_V_MAX: u32 = 2;
/// The byte representation of [`INDEX_V_MAX`]
const INDEX_V_MAX_BYTES: [u8; 4] = INDEX_V_MAX.to_le_bytes();

use crate::{CacheError, Error, IndexKrate};

pub struct ValidCacheEntry<'buffer> {
    /// The cache entry's revision
    ///
    /// For git indices this will be the sha1 of the HEAD commit when the cache
    /// entry was written
    ///
    /// For sparse indicies, this will be an HTTP header from the response that
    /// was last written to disk, which is currently either `etag: <etag>` or
    /// `last-modified: <timestamp>`
    pub revision: &'buffer str,
    /// Portion of the buffer containing the individual version entries for the
    /// cache entry
    pub version_entries: &'buffer [u8],
}

impl<'buffer> ValidCacheEntry<'buffer> {
    /// Attempts to read a cache entry from a block of bytes.
    pub fn read(mut buffer: &'buffer [u8]) -> Result<Self, CacheError> {
        let cache_version = *buffer.first().ok_or(CacheError::InvalidCacheEntry)?;

        match cache_version.cmp(&CURRENT_CACHE_VERSION) {
            std::cmp::Ordering::Less => return Err(CacheError::OutdatedCacheVersion),
            std::cmp::Ordering::Greater => return Err(CacheError::UnknownCacheVersion),
            _ => {}
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

        Ok(Self {
            revision,
            version_entries,
        })
    }

    /// Reads this cache entry into a [`Krate`]
    ///
    /// If specified, the `revision` will be used to ignore cache entries
    /// that are outdated
    pub fn to_krate(&self, revision: Option<&str>) -> Result<Option<IndexKrate>, Error> {
        if let Some(iv) = revision {
            if iv != self.revision {
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
        revision: &str,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;

        const SPLIT: &[u8] = &[0];

        let mut w = std::io::BufWriter::new(writer);
        w.write_all(&[CURRENT_CACHE_VERSION])?;
        w.write_all(&INDEX_V_MAX_BYTES)?;
        w.write_all(revision.as_bytes())?;
        w.write_all(SPLIT)?;

        for iv in &self.versions {
            let semver = iv.version.to_string();
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
