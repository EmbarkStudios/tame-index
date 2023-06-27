use super::IndexCache;
use crate::{utils::cargo_home, Error, IndexKrate, KrateName, PathBuf};

/// The URL of the crates.io index for use with git, see [`Index::with_path`]
pub const CRATES_IO_INDEX: &str = "https://github.com/rust-lang/crates.io-index";

/// Allows access to a cargo git registry index
///
/// Uses Cargo's cache.
pub struct GitIndex {
    pub(super) cache: IndexCache,
    #[allow(dead_code)]
    pub(super) url: String,
    /// The sha-1 head commit id encoded as hex
    pub head: Option<[u8; 40]>,
}

impl GitIndex {
    /// Creates a view over the sparse HTTP index from a provided URL
    ///
    /// Note this opens the same location on disk that Cargo uses for that
    /// registry index's metadata and cache.
    ///
    /// Use [`Self::with_path`] if you wish to override the disk location
    #[inline]
    pub fn with_url(url: &str) -> Result<Self, Error> {
        Self::with_path(cargo_home()?, url)
    }

    /// Creates an index for the default crates.io registry, using the same
    /// disk location as Cargo itself.
    ///
    /// This is the recommended way to access the crates.io git index.
    #[inline]
    pub fn crates_io() -> Result<Self, Error> {
        Self::with_url(CRATES_IO_INDEX)
    }

    /// Creates a view over the git index from the provided URL, rooted
    /// at the specified location
    #[inline]
    pub fn with_path(root: impl Into<PathBuf>, url: impl AsRef<str>) -> Result<Self, Error> {
        let (path, url) = crate::utils::get_index_details(url.as_ref(), Some(root.into()))?;
        Ok(Self::at_path(path, url))
    }

    /// Creates a view over the git index at the exact specified path
    #[inline]
    pub fn at_path(path: PathBuf, mut url: String) -> Self {
        if !url.ends_with('/') {
            url.push('/');
        }

        Self {
            cache: IndexCache::at_path(path),
            url,
            head: None,
        }
    }

    /// Sets the sha-1 id for the head commit.
    ///
    /// If set, this will be used to disregard cache entries that do not match
    #[inline]
    pub fn set_head_commit(&mut self, commit_id: Option<[u8; 20]>) {
        if let Some(id) = &commit_id {
            let mut hex_head = [0u8; 40];
            crate::utils::encode_hex(id, &mut hex_head);
            self.head = Some(hex_head);
        } else {
            self.head = None;
        }
    }

    /// Gets the hex-encoded sha-1 id for the head commit
    #[inline]
    pub fn head_commit(&self) -> Option<&str> {
        self.head.as_ref().map(|hc| {
            // SAFETY: the buffer is always ASCII hex
            #[allow(unsafe_code)]
            unsafe {
                std::str::from_utf8_unchecked(hc)
            }
        })
    }

    /// Reads a crate from the local cache of the index.
    ///
    /// There are no guarantees around freshness, and no network I/O will be
    /// performed.
    #[inline]
    pub fn cached_krate(&self, name: KrateName<'_>) -> Result<Option<IndexKrate>, Error> {
        self.cache.cached_krate(name, self.head_commit())
    }

    /// Writes the specified crate to the cache.
    ///
    /// Note that no I/O will be performed if [`Self::set_head_commit`] has not
    /// been set to `Some`
    #[inline]
    pub fn write_to_cache(&self, krate: &IndexKrate) -> Result<Option<PathBuf>, Error> {
        let Some(head) = self.head_commit() else { return Ok(None); };
        self.cache.write_to_cache(krate, head).map(Some)
    }
}
