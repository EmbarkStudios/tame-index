#![doc = include_str!("../README.md")]

pub mod error;
pub mod index;
pub mod krate;
pub mod utils;

pub use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};

pub use error::{CacheError, Error, HttpError, InvalidUrl, InvalidUrlError};
pub use index::{
    git::CRATES_IO_INDEX, sparse::CRATES_IO_HTTP_INDEX, GitIndex, IndexCache, SparseIndex,
};
pub use krate::{IndexDependency, IndexKrate, IndexVersion};

/// Used to wrap user-provided strings so that bad crate names are required to be handled
/// separately from things more outside the user control such as I/O errors
#[derive(Copy, Clone)]
pub struct KrateName<'name>(&'name str);

impl<'name> TryFrom<&'name str> for KrateName<'name> {
    type Error = Error;
    #[inline]
    fn try_from(s: &'name str) -> Result<Self, Self::Error> {
        if s.is_empty() {
            Err(Error::EmptyCrateName)
        } else if !s.is_ascii() {
            Err(Error::NonAsciiCrateName)
        } else {
            Ok(Self(s))
        }
    }
}

impl<'name> KrateName<'name> {
    /// Writes the crate's prefix to the specified string
    ///
    /// Cargo uses a simple prefix in the registry index so that crate's can be
    /// partitioned, particularly on disk without running up against potential OS
    /// specific issues when hundreds of thousands of files are located with a single
    /// directory
    ///
    /// The separator should be [`std::path::MAIN_SEPARATOR`] in disk cases and
    /// '/' when used for urls
    pub fn prefix(&self, acc: &mut String, sep: char) {
        let name = self.0;

        match name.len() {
            0 => unreachable!(),
            1 => acc.push('1'),
            2 => acc.push('2'),
            3 => {
                acc.push('3');
                acc.push(sep);
                acc.push_str(&name[..1]);
            }
            _ => {
                acc.push_str(&name[..2]);
                acc.push(sep);
                acc.push_str(&name[2..4]);
            }
        }
    }

    /// Gets the relative path to a crate
    ///
    /// This will be of the form [`Self::prefix`] + `<sep>` + `<name>`
    ///
    /// If not specified, the separator is [`std::path::MAIN_SEPARATOR`]
    ///
    /// ```
    /// let crate_name: tame_index::KrateName = "tame-index".try_into().unwrap();
    /// assert_eq!(crate_name.relative_path(Some('/')), "ta/me/tame-index");
    /// ```
    pub fn relative_path(&self, sep: Option<char>) -> String {
        let name = self.0;
        // Preallocate with the maximum possible width of a crate prefix `aa/bb/`
        let mut rel_path = String::with_capacity(name.len() + 6);
        let sep = sep.unwrap_or(std::path::MAIN_SEPARATOR);

        self.prefix(&mut rel_path, sep);
        rel_path.push(sep);
        rel_path.push_str(name);

        rel_path
    }
}

use std::fmt;

impl<'k> fmt::Display for KrateName<'k> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl<'k> fmt::Debug for KrateName<'k> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
