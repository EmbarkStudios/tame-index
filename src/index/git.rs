use super::{FileLock, IndexCache};
use crate::{Error, IndexKrate, KrateName, PathBuf, utils};

/// The URL of the crates.io index for use with git
pub const CRATES_IO_INDEX: &str = "https://github.com/rust-lang/crates.io-index";

pub type HexBytes = [u8; 40];

/// Hex encoded sha1
#[derive(Copy, Clone)]
pub struct Sha1Hex(HexBytes);

impl Sha1Hex {
    #[inline]
    pub(crate) fn internal(v: HexBytes) -> Self {
        Self(v)
    }

    pub(crate) fn empty() -> Self {
        Self([0u8; _])
    }
}

impl AsRef<[u8]> for Sha1Hex {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<str> for Sha1Hex {
    #[allow(unsafe_code)]
    fn as_ref(&self) -> &str {
        // SAFETY: we only allow conversion via encode_hex which only writes hex digits
        unsafe { std::str::from_utf8_unchecked(&self.0) }
    }
}

impl From<[u8; 20]> for Sha1Hex {
    fn from(value: [u8; 20]) -> Self {
        let mut hex = Self::empty();
        utils::encode_hex(&value, &mut hex.0);
        hex
    }
}

pub enum GitRevision {
    Raw([u8; 20]),
    Hex(Sha1Hex),
}

impl PartialEq for GitRevision {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Raw(a), Self::Raw(b)) => a == b,
            (Self::Hex(a), Self::Hex(b)) => a.0 == b.0,
            (Self::Raw(a), Self::Hex(b)) => utils::compare_hex(a, &b.0),
            (Self::Hex(a), Self::Raw(b)) => utils::compare_hex(b, &a.0),
        }
    }
}

/// Allows access to a cargo git registry index
///
/// Uses Cargo's cache.
pub struct GitIndex {
    pub(super) cache: IndexCache,
    #[allow(dead_code)]
    pub(super) url: String,
    /// The sha-1 head commit id encoded as hex
    pub head: Option<Sha1Hex>,
}

impl GitIndex {
    /// Creates a new git index for the specified location
    #[inline]
    pub fn new(il: crate::index::IndexLocation<'_>) -> Result<Self, Error> {
        if il.url.is_sparse() {
            return Err(crate::InvalidUrl {
                url: il.url.as_str().to_owned(),
                source: crate::InvalidUrlError::SparseForGit,
            }
            .into());
        }

        let (path, url) = il.into_parts()?;
        Ok(Self {
            cache: IndexCache::at_path(path),
            url,
            head: None,
        })
    }

    /// Sets the sha-1 id for the head commit.
    ///
    /// If set, this will be used to disregard cache entries that do not match
    #[inline]
    pub fn set_head_commit(&mut self, commit_id: Option<[u8; 20]>) {
        self.head = commit_id.map(Sha1Hex::from);
    }

    /// Gets the hex-encoded sha-1 id for the head commit
    #[inline]
    pub fn head_commit(&self) -> Option<&Sha1Hex> {
        self.head.as_ref()
    }

    /// Reads a crate from the local cache of the index.
    ///
    /// There are no guarantees around freshness, and no network I/O will be
    /// performed.
    #[inline]
    pub fn cached_krate(
        &self,
        name: KrateName<'_>,
        lock: &FileLock,
    ) -> Result<Option<IndexKrate>, Error> {
        self.cache.cached_krate(
            name,
            self.head_commit()
                .map(|sha1| crate::index::cache::Revision::Git(GitRevision::Hex(*sha1))),
            lock,
        )
    }

    /// Writes the specified crate to the cache.
    ///
    /// Note that no I/O will be performed if `blob_id` or [`Self::set_head_commit`]
    /// has not been set to `Some`
    #[inline]
    pub fn write_to_cache(
        &self,
        krate: &IndexKrate,
        blob_id: Option<Sha1Hex>,
        lock: &FileLock,
    ) -> Result<Option<PathBuf>, Error> {
        let Some(id) = blob_id.or_else(|| self.head_commit().copied()) else {
            return Ok(None);
        };
        self.cache
            .write_to_cache(
                krate,
                &crate::index::cache::Revision::Git(GitRevision::Hex(id)),
                lock,
            )
            .map(Some)
    }
}
