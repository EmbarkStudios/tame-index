#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

pub mod error;
pub mod flock;
pub mod index;
pub mod krate;
mod krate_name;
pub mod utils;

pub use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};

pub use error::{CacheError, Error, HttpError, InvalidUrl, InvalidUrlError};
pub use index::{
    GitIndex, IndexCache, IndexLocation, IndexPath, IndexUrl, SparseIndex, cache::Revision,
    git::CRATES_IO_INDEX, sparse::CRATES_IO_HTTP_INDEX, sparse::EtagCache,
};
pub use krate::{IndexDependency, IndexKrate, IndexVersion};
pub use krate_name::KrateName;
pub use semver::Version;

/// Reexports of some crates for easier downstream usage without requiring adding
/// your own dependencies
pub mod external {
    #[cfg(feature = "local-builder")]
    pub use bytes;
    pub use http;
}
