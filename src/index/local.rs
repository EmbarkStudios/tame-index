//! Contains code for reading and writing [local registries](https://doc.rust-lang.org/cargo/reference/source-replacement.html#local-registry-sources)

#[cfg(feature = "local-builder")]
pub mod builder;

use super::FileLock;
use crate::{Error, KrateName, Path, PathBuf};

/// A local index, in the same layout as a git registry or spare registry
///
/// For [`LocalRegistry`] this is `${local_registry_root}/index`
pub struct LocalIndex {
    path: PathBuf,
}

impl LocalIndex {
    /// Opens a [`Self`]
    ///
    /// This method will open either the provided path, or the `index` subdirectory if it exists
    #[inline]
    pub fn open(mut root: PathBuf) -> Result<Self, Error> {
        if root.ends_with("index") {
            return Ok(Self { path: root });
        }

        root.push("index");

        match std::fs::metadata(&root) {
            Ok(md) => {
                if !md.is_dir() {
                    return Err(Error::IoPath(
                        std::io::ErrorKind::NotADirectory.into(),
                        root,
                    ));
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                root.pop();
            }
            Err(err) => {
                return Err(Error::IoPath(err, root));
            }
        }

        Ok(Self { path: root })
    }

    /// Gets the index information for the crate
    #[inline]
    pub fn krate<'l>(
        &self,
        name: KrateName<'_>,
        lock: &'l FileLock,
    ) -> Result<Option<super::IndexEntry<'_, 'l>>, Error> {
        let index_path = self.krate_path(name);

        let mmap = match super::LockedMmap::mmap(&index_path, lock) {
            Ok(f) => f,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(Error::IoPath(err, index_path)),
        };

        Ok(Some(super::IndexEntry::Local(mmap)))
    }

    /// Gets the path to the index entry for the krate.
    ///
    /// Note that unlike .cache entries for git and sparse indices, these are not
    /// binary files, they are just the JSON line format
    #[inline]
    pub fn krate_path(&self, name: KrateName<'_>) -> PathBuf {
        make_path(&self.path, name, false)
    }
}

/// A [local registry](https://doc.rust-lang.org/cargo/reference/source-replacement.html#local-registry-sources)
/// implementation
pub struct LocalRegistry {
    index: LocalIndex,
}

impl LocalRegistry {
    /// Opens an existing local registry, optionally validating it if the `local-builder` feature is enabled
    pub fn open(mut path: PathBuf, _validate: bool) -> Result<Self, Error> {
        // There _should_ only be one directory in the root path, `index`
        path.push("index");

        match std::fs::metadata(&path) {
            Ok(md) => {
                if !md.is_dir() {
                    return Err(Error::IoPath(
                        std::io::ErrorKind::NotADirectory.into(),
                        path,
                    ));
                }
            }
            Err(err) => {
                return Err(Error::IoPath(err, path));
            }
        }

        let index = LocalIndex { path };

        #[cfg(feature = "local-builder")]
        {
            if _validate {
                Self::validate(&index)?;
            }
        }

        Ok(Self { index })
    }

    /// Gets the index for this registry
    #[inline]
    pub fn index(&self) -> &LocalIndex {
        &self.index
    }
}

#[inline]
fn make_path(root: &Path, name: KrateName<'_>, with_index: bool) -> PathBuf {
    let rel_path = name.relative_path(None);

    let mut index_path = PathBuf::with_capacity(
        root.as_str().len() + if with_index { 7 } else { 1 } + rel_path.len(),
    );
    index_path.push(root);
    if with_index {
        index_path.push("index");
    }
    index_path.push(rel_path);

    index_path
}
