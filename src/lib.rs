#![doc = include_str!("../README.md")]

pub mod error;
pub mod index;
pub mod krate;
mod krate_name;
pub mod utils;

pub use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};

pub use error::{CacheError, Error, HttpError, InvalidUrl, InvalidUrlError};
pub use index::{
    git::CRATES_IO_INDEX, sparse::CRATES_IO_HTTP_INDEX, GitIndex, IndexCache, IndexLocation,
    IndexPath, IndexUrl, SparseIndex,
};
pub use krate::{IndexDependency, IndexKrate, IndexVersion};
pub use krate_name::KrateName;
