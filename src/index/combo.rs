use crate::{
    index::{RemoteGitIndex, RemoteSparseIndex},
    Error, IndexKrate, KrateName,
};

/// A wrapper around either a [`RemoteGitIndex`] or [`RemoteSparseIndex`]
pub enum ComboIndex {
    /// A standard git based registry index. No longer the default for crates.io
    /// as of 1.70.0
    Git(RemoteGitIndex),
    /// An HTTP sparse index
    Sparse(RemoteSparseIndex),
}

impl ComboIndex {
    /// Retrieves the index metadata for the specified crate name, optionally
    /// writing a cache entry for it if there was not already an up to date one
    #[inline]
    pub fn krate(
        &self,
        name: KrateName<'_>,
        write_cache_entry: bool,
    ) -> Result<Option<IndexKrate>, Error> {
        match self {
            Self::Git(index) => index.krate(name, write_cache_entry),
            Self::Sparse(index) => index.krate(name, write_cache_entry),
        }
    }

    /// Retrieves the cached crate metadata if it exists
    #[inline]
    pub fn cached_krate(&self, name: KrateName<'_>) -> Result<Option<IndexKrate>, Error> {
        match self {
            Self::Git(index) => index.cached_krate(name),
            Self::Sparse(index) => index.cached_krate(name),
        }
    }
}

impl From<RemoteGitIndex> for ComboIndex {
    #[inline]
    fn from(index: RemoteGitIndex) -> Self {
        Self::Git(index)
    }
}

impl From<RemoteSparseIndex> for ComboIndex {
    #[inline]
    fn from(index: RemoteSparseIndex) -> Self {
        Self::Sparse(index)
    }
}
