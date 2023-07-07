//! Helpers for initializing the remote and local disk location of an index

use crate::{Error, PathBuf};

/// A remote index url
#[derive(Default)]
pub enum IndexUrl<'iu> {
    /// The canonical crates.io HTTP sparse index.
    ///
    /// See [`sparse::CRATES_IO_HTTP_INDEX`]
    #[default]
    CratesIoSparse,
    /// The canonical crates.io git index.
    ///
    /// See [`git::CRATES_IO_INDEX`]
    CratesIoGit,
    /// A non-crates.io index.
    ///
    /// This variant uses the url to determine the index kind (sparse or git) by
    /// inspecting the url's scheme. This is because sparse indices are required
    /// to have the `sparse+` scheme modifier
    NonCratesIo(&'iu str),
}

impl<'iu> IndexUrl<'iu> {
    /// Gets the url as a string
    pub fn as_str(&self) -> &'iu str {
        match self {
            Self::CratesIoSparse => crate::CRATES_IO_HTTP_INDEX,
            Self::CratesIoGit => crate::CRATES_IO_INDEX,
            Self::NonCratesIo(url) => url,
        }
    }

    /// Returns true if the url points to a sparse registry
    pub fn is_sparse(&self) -> bool {
        match self {
            Self::CratesIoSparse => true,
            Self::CratesIoGit => false,
            Self::NonCratesIo(url) => url.starts_with("sparse+http"),
        }
    }
}

/// The local disk location to place an index
#[derive(Default)]
pub enum IndexPath {
    /// The default cargo home root path
    #[default]
    CargoHome,
    /// User-specified root path
    UserSpecified(PathBuf),
    /// An exact path on disk where an index is located.
    ///
    /// Unlike the other two variants, this variant won't take the index's url
    /// into account to calculate the unique url hash as part of the full path
    Exact(PathBuf),
}

impl From<Option<PathBuf>> for IndexPath {
    /// Converts an optional path to a rooted path.
    ///
    /// This never constructs a [`Self::Exact`], that can only be done explicitly
    fn from(pb: Option<PathBuf>) -> Self {
        if let Some(pb) = pb {
            Self::UserSpecified(pb)
        } else {
            Self::CargoHome
        }
    }
}

/// Helper for constructing an index location, consisting of the remote url for
/// the index and the local location on disk
#[derive(Default)]
pub struct IndexLocation<'il> {
    /// The remote url of the registry index
    pub url: IndexUrl<'il>,
    /// The local disk path of the index
    pub root: IndexPath,
}

impl<'il> IndexLocation<'il> {
    /// Constructs an index with the specified url located in the default cargo
    /// home
    pub fn new(url: IndexUrl<'il>) -> Self {
        Self {
            url,
            root: IndexPath::CargoHome,
        }
    }

    /// Changes the root location of the index on the local disk.
    ///
    /// If not called, or set to [`None`], the default cargo home disk location
    /// is used as the root
    pub fn with_root(mut self, root: Option<PathBuf>) -> Self {
        self.root = root.into();
        self
    }

    /// Obtains the full local disk path and URL of this index location
    pub fn into_parts(self) -> Result<(PathBuf, String), Error> {
        let url = self.url.as_str();

        let root = match self.root {
            IndexPath::CargoHome => crate::utils::cargo_home()?,
            IndexPath::UserSpecified(root) => root,
            IndexPath::Exact(path) => return Ok((path, url.to_owned())),
        };

        let (path, mut url) = crate::utils::get_index_details(url, Some(root))?;

        if !url.ends_with('/') {
            url.push('/');
        }

        Ok((path, url))
    }
}
