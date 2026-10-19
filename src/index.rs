//! Provides functionality for interacting with both local and remote registry
//! indices

pub mod cache;
pub mod git;
pub mod local;
pub mod location;
pub mod sparse;

pub use cache::IndexCache;
pub use git::GitIndex;
pub use local::{LocalIndex, LocalRegistry};
pub use location::{IndexLocation, IndexPath, IndexUrl};
pub use sparse::SparseIndex;

pub use crate::flock::FileLock;

/// Global configuration of an index, reflecting the [contents of config.json](https://doc.rust-lang.org/cargo/reference/registries.html#index-format).
#[derive(Eq, PartialEq, PartialOrd, Ord, Clone, Debug)]
pub struct IndexConfig {
    /// Pattern for creating download URLs. See [`Self::download_url`].
    pub dl: String,
    /// Base URL for publishing, etc.
    pub api: Option<String>,
    /// Indicates whether this is a private registry that requires all
    /// operations to be authenticated including API requests, crate downloads
    /// and sparse index updates.
    pub auth_required: bool,
}

impl<'de> serde::Deserialize<'de> for IndexConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct V;

        impl<'de> serde::de::Visitor<'de> for V {
            type Value = IndexConfig;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("index configuration")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut dl = None;
                let mut api = None;
                let mut auth_required = None;

                while let Some(k) = map.next_key::<std::borrow::Cow<'_, str>>()? {
                    match k.as_ref() {
                        "dl" => dl = Some(map.next_value()?),
                        "api" => api = Some(map.next_value()?),
                        "auth-required" => auth_required = Some(map.next_value()?),
                        _ => {
                            // We could error here, but _probably_ nicer to just ignore new/invalid fields to avoid
                            // _probably_ inconsequential future changes (at least when they immediately come out)
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }

                Ok(IndexConfig {
                    dl: dl.ok_or_else(|| serde::de::Error::missing_field("dl"))?,
                    api,
                    auth_required: auth_required.unwrap_or_default(),
                })
            }
        }

        deserializer.deserialize_any(V)
    }
}

impl IndexConfig {
    /// Gets the download url for the specified crate version
    ///
    /// See <https://doc.rust-lang.org/cargo/reference/registries.html#index-format>
    /// for more info
    pub fn download_url(&self, name: crate::KrateName<'_>, version: &str) -> String {
        // Special case crates.io which will easily be the most common case in
        // almost all scenarios, we just use the _actual_ url directly, which
        // avoids a 301 redirect, though obviously this will be bad if crates.io
        // ever changes the redirect, this has been stable since 1.0 (at least)
        // so it's unlikely to ever change, and if it does, it would be easy to
        // update, though obviously would be broken on previously published versions
        if self.dl == "https://crates.io/api/v1/crates" {
            return format!("https://static.crates.io/crates/{name}/{name}-{version}.crate");
        }

        let mut dl = self.dl.clone();

        if dl.contains('{') {
            while let Some(start) = dl.find("{crate}") {
                dl.replace_range(start..start + 7, name.0);
            }

            while let Some(start) = dl.find("{version}") {
                dl.replace_range(start..start + 9, version);
            }

            if dl.contains("{prefix}") || dl.contains("{lowerprefix}") {
                let mut prefix = String::with_capacity(6);
                name.prefix(&mut prefix, '/');

                while let Some(start) = dl.find("{prefix}") {
                    dl.replace_range(start..start + 8, &prefix);
                }

                if dl.contains("{lowerprefix}") {
                    prefix.make_ascii_lowercase();

                    while let Some(start) = dl.find("{lowerprefix}") {
                        dl.replace_range(start..start + 13, &prefix);
                    }
                }
            }
        } else {
            // If none of the markers are present, then the value /{crate}/{version}/download is appended to the end
            if !dl.ends_with('/') {
                dl.push('/');
            }

            dl.push_str(name.0);
            dl.push('/');
            dl.push_str(version);
            dl.push('/');
            dl.push_str("download");
        }

        dl
    }
}

use crate::Error;

/// Provides simpler access to the cache for an index, regardless of the registry kind
#[non_exhaustive]
pub enum ComboIndexCache {
    /// A git index
    Git(GitIndex),
    /// A sparse HTTP index
    Sparse(SparseIndex),
    /// A local index on disk, either a bare index, or the `index` sub-directory of a [`LocalRegistry`]
    Local(LocalIndex),
    /// A local registry, which pairs a [`LocalIndex`] with a set of `.crate` tarballs
    LocalRegistry(LocalRegistry),
}

impl ComboIndexCache {
    /// Retrieves the index metadata for the specified crate name
    #[inline]
    pub fn cached_krate<'l>(
        &self,
        name: crate::KrateName<'_>,
        lock: &'l FileLock,
    ) -> Result<Option<IndexEntry<'_, 'l>>, Error> {
        match self {
            Self::Git(index) => index.cached_krate(name, lock),
            Self::Sparse(index) => index.cached_krate(name, lock),
            Self::Local(li) => li.krate(name, lock),
            Self::LocalRegistry(lr) => lr.index().krate(name, lock),
        }
    }

    /// Gets the path to the cache entry for the specified crate
    pub fn cache_path(&self, name: crate::KrateName<'_>) -> crate::PathBuf {
        match self {
            Self::Git(index) => index.cache.cache_path(name),
            Self::Sparse(index) => index.cache().cache_path(name),
            Self::Local(lr) => lr.krate_path(name),
            Self::LocalRegistry(lr) => lr.index().krate_path(name),
        }
    }

    /// Constructs a [`Self`] for the specified index.
    ///
    /// See [`Self::crates_io`] if you want to create a crates.io index based
    /// upon other information in the user's environment
    ///
    /// If the [`Indexlocation`] is local, it will only create a [`Self::Local`], a [`Self::LocalRegistry`] must be
    /// created manually
    pub fn new(il: IndexLocation<'_>) -> Result<Self, Error> {
        let index = if il.url.is_sparse() {
            let sparse = SparseIndex::new(il)?;
            Self::Sparse(sparse)
        } else if let IndexUrl::Local(path) = il.url {
            Self::Local(LocalIndex::open(path.into_owned())?)
        } else {
            let git = GitIndex::new(il)?;
            Self::Git(git)
        };

        Ok(index)
    }
}

impl From<SparseIndex> for ComboIndexCache {
    #[inline]
    fn from(si: SparseIndex) -> Self {
        Self::Sparse(si)
    }
}

impl From<GitIndex> for ComboIndexCache {
    #[inline]
    fn from(gi: GitIndex) -> Self {
        Self::Git(gi)
    }
}

/// A `mmap` that is associated with the lifetime of a [`FileLock`]
pub struct LockedMmap<'l> {
    #[allow(dead_code)]
    lock: &'l FileLock,
    mmap: memmap2::Mmap,
}

impl<'l> LockedMmap<'l> {
    /// Mmaps the specified file path, associating it with the provided lock
    #[inline]
    #[allow(unsafe_code)]
    pub fn mmap(path: &crate::Path, lock: &'l FileLock) -> std::io::Result<Self> {
        let file = std::fs::File::open(path)?;
        // SAFETY: we pair the lifetime of the lock with mmap, to hopefully prevent faults due to the cache
        // file changing from underneath us, at least with other actors that respect the file lock
        let mmap = unsafe { memmap2::Mmap::map(&file)? };

        Ok(Self { lock, mmap })
    }
}

/// An index entry from a remote index
#[derive(Copy, Clone)]
pub struct JsonLines<'r>(&'r [u8]);

impl<'r> JsonLines<'r> {
    /// Performs basic validation that this is a sequence of one or more JSON objects separated by newlines
    #[inline]
    pub fn validate(mut re: &'r [u8]) -> Result<Self, crate::error::InvalidJsonLines> {
        let mut line = 0;

        // there should only be one trailing newline from a valid response, remove all of them though so we don't need
        // to special case the last empty line
        while re.last() == Some(&b'\n') {
            re = &re[..re.len() - 1];
        }

        for item in crate::utils::split(re, b'\n') {
            if item.len() < 2 || item[0] != b'{' || item[item.len() - 1] != b'}' {
                return Err(crate::error::InvalidJsonLines::NotJson(line));
            }

            line += 1;
        }

        if line != 0 {
            Ok(Self(re))
        } else {
            Err(crate::error::InvalidJsonLines::Empty)
        }
    }
}

/// An index entry, acquired from either a remote index or a local cache entry
pub enum IndexEntry<'r, 'l> {
    /// Data retrieved from a remote source either a sparse index or a git index
    ///
    /// This is a series of JSON objects, 1 for each published version, separated by newlines
    Remote(JsonLines<'r>),
    /// Data retrieved from a local entry in the registry's `.cache` directory
    ///
    /// This is a series of tuples of semver and the same JSON object as found in the remote index, separated by `\0`
    Cache(LockedMmap<'l>),
    /// Data retrieved from a local registry
    ///
    /// This is the same JSON lines format as a remote registry
    Local(LockedMmap<'l>),
}

use crate::{IndexKrate, Revision, utils};

impl IndexEntry<'_, '_> {
    /// Deserializes all versions in this entry into an [`IndexKrate`]
    #[inline]
    pub fn deserialize(&self) -> Result<IndexKrate, Error> {
        match self {
            Self::Remote(r) => IndexKrate::from_slice(r.0),
            Self::Cache(mmap) => {
                let entry = cache::ValidCacheEntry::read(&mmap.mmap)?;
                IndexKrate::from_cache(utils::split(entry.version_entries, 0))
            }
            Self::Local(mmap) => IndexKrate::from_slice(&mmap.mmap),
        }
    }

    /// The same as [`Self::into_krate`] except `Ok(None)` is returned if this is a cache entry and its revision does
    /// not match the specified revision
    #[inline]
    pub fn into_krate_with_revision(
        self,
        revision: Revision<'_>,
    ) -> Result<Option<IndexKrate>, Error> {
        if let Some(centry) = self.cached()? {
            if centry.revision != revision {
                return Ok(None);
            }
        }

        self.try_into().map(Some)
    }

    /// Gets self as a [`cache::ValidCacheEntry`] if it is one
    #[inline]
    pub fn cached(&self) -> Result<Option<cache::ValidCacheEntry<'_>>, crate::error::CacheError> {
        match self {
            Self::Cache(c) => cache::ValidCacheEntry::read(&c.mmap).map(Some),
            Self::Remote(_) | Self::Local(_) => Ok(None),
        }
    }

    /// Creates an iterator over each version in the index entry
    #[inline]
    pub fn iter(&self) -> Result<impl Iterator<Item = IndexEntryVersion<'_>>, Error> {
        match self {
            Self::Remote(r) => Ok(IndexEntryIter::Remote(utils::split(r.0, b'\n'))),
            Self::Cache(c) => {
                let entry = cache::ValidCacheEntry::read(&c.mmap)?;
                Ok(IndexEntryIter::Cache(utils::split(
                    entry.version_entries,
                    0,
                )))
            }
            Self::Local(l) => Ok(IndexEntryIter::Remote(utils::split(&l.mmap, b'\n'))),
        }
    }
}

impl TryFrom<IndexEntry<'_, '_>> for IndexKrate {
    type Error = Error;

    /// Deserializes the entry into an [`IndexKrate`]
    fn try_from(value: IndexEntry<'_, '_>) -> Result<Self, Self::Error> {
        value.deserialize()
    }
}

enum IndexEntryIter<'r> {
    Remote(utils::Split<'r>),
    Cache(utils::Split<'r>),
}

impl<'r> Iterator for IndexEntryIter<'r> {
    type Item = IndexEntryVersion<'r>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Remote(r) => r.next().map(|json| IndexEntryVersion {
                version: None,
                json,
            }),
            Self::Cache(c) => {
                let version = c.next()?;
                let json = c.next()?;

                Some(IndexEntryVersion {
                    version: Some(version),
                    json,
                })
            }
        }
    }
}

/// A single version in the [`IndexEntry`]
pub struct IndexEntryVersion<'r> {
    version: Option<&'r [u8]>,
    json: &'r [u8],
}

impl IndexEntryVersion<'_> {
    /// Deserializes the entire [`crate::krate::IndexVersion`]
    #[inline]
    pub fn deserialize(&self) -> Result<crate::krate::IndexVersion, Error> {
        serde_json::from_slice(self.json).map_err(Error::from)
    }

    #[inline]
    fn find_version(&self, split: &memchr::memmem::Finder<'_>) -> Option<&[u8]> {
        // the JSON in the index is (currently) {name:,vers:,deps:[..],..}, so we just assume it is always the second field
        let mut start = 0;

        for end in memchr::memchr_iter(b',', self.json) {
            let split = split.find(&self.json[start..end])?;

            let key = &self.json[start..start + split];

            if key.ends_with(b"\"vers\"") {
                let value = &self.json[start + split + 1..end];
                return value
                    .strip_prefix(b" ")
                    .unwrap_or(value)
                    .strip_prefix(b"\"")?
                    .strip_suffix(b"\"");
            }

            start = end + 1;
        }

        None
    }

    /// Retrieves the version and json raw bytes
    #[inline]
    pub fn raw(&self) -> (&str, &[u8]) {
        let version = if let Some(v) = self.version {
            v
        } else {
            let finder = memchr::memmem::Finder::new(&b":");
            self.find_version(&finder).unwrap_or(b"")
        };

        (std::str::from_utf8(version).unwrap_or(""), self.json)
    }

    /// Retrieves only the version and the yanked status of this entry
    pub fn yanked(self) -> Result<(semver::Version, bool), Error> {
        let mut iter = memchr::memchr2_iter(b',', b'}', self.json);
        let finder = memchr::memmem::Finder::new(&b":");

        let version = if let Some(v) = self.version {
            v
        } else {
            self.find_version(&finder).unwrap_or(b"")
        };

        let version = std::str::from_utf8(version).unwrap_or("").parse()?;

        // Conversely `yanked` is one of the last fields, so we search from the end, currently this field is always
        // present though there could be a future where it is only present when the version has been yanked, thus we stop
        // searching and assume false once we hit a '}' with the assumption that the overall structure will stay the same
        // and this indicates the end of the `deps` object. If that isn't the case and yanked moves to before the `deps`
        // section then we could add advancement of the iterator from where it left off if we already parsed the version
        if iter.next_back().map_or(b'%', |i| self.json[i]) != b'}' {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "expected version to end with '}' but it did not",
            )));
        }

        let mut end = self.json.len() - 1;
        let yanked = loop {
            let Some(next) = iter.next_back() else {
                break false;
            };

            if self.json[next] == b'}' {
                break false;
            }

            let item = &self.json[next + 1..end];
            end = next;

            let Some(split) = finder.find(item) else {
                continue;
            };
            let (key, value) = item.split_at(split);
            if !key.ends_with(b"\"yanked\"") {
                continue;
            }

            match value[1..].strip_prefix(b" ").unwrap_or(&value[1..]) {
                b"false" => break false,
                b"true" => break true,
                _ => {
                    return Err(Error::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "yanked field did not contain a valid boolean value",
                    )));
                }
            }
        };

        Ok((version, yanked))
    }
}

#[cfg(test)]
mod test {
    use super::IndexConfig;

    macro_rules! kn {
        ($n:literal) => {
            crate::KrateName::from_static($n)
        };
    }

    /// Validates we get the non-redirect url for crates.io downloads
    #[test]
    fn download_url_crates_io() {
        let crates_io = IndexConfig {
            dl: "https://crates.io/api/v1/crates".into(),
            api: Some("https://crates.io".into()),
            auth_required: false,
        };

        assert_eq!(
            crates_io.download_url(kn!("a"), "1.0.0"),
            "https://static.crates.io/crates/a/a-1.0.0.crate"
        );
        assert_eq!(
            crates_io.download_url(kn!("aB"), "0.1.0"),
            "https://static.crates.io/crates/aB/aB-0.1.0.crate"
        );
        assert_eq!(
            crates_io.download_url(kn!("aBc"), "0.1.0"),
            "https://static.crates.io/crates/aBc/aBc-0.1.0.crate"
        );
        assert_eq!(
            crates_io.download_url(kn!("aBc-123"), "0.1.0"),
            "https://static.crates.io/crates/aBc-123/aBc-123-0.1.0.crate"
        );
    }

    /// Validates we get a simple non-crates.io download
    #[test]
    fn download_url_non_crates_io() {
        let ic = IndexConfig {
            dl: "https://dl.cloudsmith.io/public/embark/deny/cargo/{crate}-{version}.crate".into(),
            api: Some("https://cargo.cloudsmith.io/embark/deny".into()),
            auth_required: false,
        };

        assert_eq!(
            ic.download_url(kn!("a"), "1.0.0"),
            "https://dl.cloudsmith.io/public/embark/deny/cargo/a-1.0.0.crate"
        );
        assert_eq!(
            ic.download_url(kn!("aB"), "0.1.0"),
            "https://dl.cloudsmith.io/public/embark/deny/cargo/aB-0.1.0.crate"
        );
        assert_eq!(
            ic.download_url(kn!("aBc"), "0.1.0"),
            "https://dl.cloudsmith.io/public/embark/deny/cargo/aBc-0.1.0.crate"
        );
        assert_eq!(
            ic.download_url(kn!("aBc-123"), "0.1.0"),
            "https://dl.cloudsmith.io/public/embark/deny/cargo/aBc-123-0.1.0.crate"
        );
    }

    /// Validates we get a more complicated non-crates.io download, exercising all
    /// of the possible replacement components
    #[test]
    fn download_url_complex() {
        let ic = IndexConfig {
            dl: "https://complex.io/ohhi/embark/rust/cargo/{lowerprefix}/{crate}/{crate}/{prefix}-{version}".into(),
            api: None,
            auth_required: false,
        };

        assert_eq!(
            ic.download_url(kn!("a"), "1.0.0"),
            "https://complex.io/ohhi/embark/rust/cargo/1/a/a/1-1.0.0"
        );
        assert_eq!(
            ic.download_url(kn!("aB"), "0.1.0"),
            "https://complex.io/ohhi/embark/rust/cargo/2/aB/aB/2-0.1.0"
        );
        assert_eq!(
            ic.download_url(kn!("ABc"), "0.1.0"),
            "https://complex.io/ohhi/embark/rust/cargo/3/a/ABc/ABc/3/A-0.1.0"
        );
        assert_eq!(
            ic.download_url(kn!("aBc-123"), "0.1.0"),
            "https://complex.io/ohhi/embark/rust/cargo/ab/c-/aBc-123/aBc-123/aB/c--0.1.0"
        );
    }
}
