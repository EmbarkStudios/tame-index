//! Utilities for building and/or validating a [local registry](https://doc.rust-lang.org/cargo/reference/source-replacement.html#local-registry-sources)

use super::LocalRegistry;
use crate::{Error, IndexKrate, PathBuf};
use smol_str::SmolStr;

/// An error that can occur when validating or creating a [`LocalRegistry`]
#[derive(Debug, thiserror::Error)]
pub enum LocalRegistryError {
    /// A .crate file has a version that is not in the index
    #[error("missing version {version} for crate {name}")]
    MissingVersion {
        /// The name of the crate
        name: String,
        /// The specific crate version
        version: SmolStr,
    },
    /// A .crate file's checksum did not match the checksum in the index for that version
    #[error("checksum mismatch for {name}-{version}.crate")]
    ChecksumMismatch {
        /// The name of the crate
        name: String,
        /// The specific crate version
        version: SmolStr,
    },
}

/// Allows the building of a local registry
pub struct LocalRegistryBuilder {
    path: PathBuf,
}

impl LocalRegistryBuilder {
    /// Creates a builder for the specified directory.
    ///
    /// The directory is required to be empty, but it will
    /// be created if it doesn't exist
    pub fn create(path: PathBuf) -> Result<Self, Error> {
        if path.exists() {
            let count = std::fs::read_dir(&path)?.count();
            if count != 0 {
                return Err(Error::IoPath(
                    std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        format!("{count} entries already exist at the specified path"),
                    ),
                    path,
                ));
            }
        } else {
            std::fs::create_dir_all(&path)?;
        }

        std::fs::create_dir_all(path.join("index"))?;

        Ok(Self { path })
    }

    /// Inserts the specified crate index entry and one or more crates files
    /// into the registry
    ///
    /// This will fail if the specified crate is already located in the index, it
    /// is your responsibility to insert the crate and all the versions you want
    /// only once
    pub fn insert(&self, krate: &IndexKrate, krates: &[ValidKrate<'_>]) -> Result<u64, Error> {
        let index_path = super::make_path(&self.path, krate.name().try_into()?, true);

        if index_path.exists() {
            return Err(Error::IoPath(
                std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "crate has already been inserted",
                ),
                index_path,
            ));
        }

        let mut written = {
            if let Err(err) = std::fs::create_dir_all(index_path.parent().unwrap()) {
                return Err(Error::IoPath(err, index_path));
            }

            let mut index_entry =
                std::fs::File::create(&index_path).map_err(|err| Error::IoPath(err, index_path))?;
            krate.write_json_lines(&mut index_entry)?;
            // This _should_ never fail, but even if it does, just ignore it
            use std::io::Seek;
            index_entry.stream_position().unwrap_or_default()
        };

        for krate in krates {
            let krate_fname = format!("{}-{}.crate", krate.iv.name, krate.iv.version);
            let krate_path = self.path.join(krate_fname);

            std::fs::write(&krate_path, &krate.buff)
                .map_err(|err| Error::IoPath(err, krate_path))?;

            written += krate.buff.len() as u64;
        }

        Ok(written)
    }

    /// Consumes the builder and opens a [`LocalRegistry`]
    #[inline]
    pub fn finalize(self, validate: bool) -> Result<LocalRegistry, Error> {
        LocalRegistry::open(self.path, validate)
    }
}

/// A wrapper around the raw byte buffer for a .crate response from a remote
/// index
pub struct ValidKrate<'iv> {
    buff: bytes::Bytes,
    iv: &'iv crate::IndexVersion,
}

impl<'iv> ValidKrate<'iv> {
    /// Given a buffer, validates its checksum matches the specified version
    pub fn validate(
        buff: impl Into<bytes::Bytes>,
        expected: &'iv crate::IndexVersion,
    ) -> Result<Self, Error> {
        let buff = buff.into();

        let computed = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&buff);
            hasher.finalize()
        };

        if computed.as_slice() != expected.checksum.0 {
            return Err(LocalRegistryError::ChecksumMismatch {
                name: expected.name.to_string(),
                version: expected.version.clone(),
            }
            .into());
        }

        Ok(Self { buff, iv: expected })
    }
}

/// Ensures the specified stream's sha-256 matches the specified checksum
#[inline]
pub fn validate_checksum<const N: usize>(
    mut stream: impl std::io::Read,
    chksum: &crate::krate::Chksum,
) -> Result<bool, std::io::Error> {
    use sha2::{Digest, Sha256};

    let mut buffer = [0u8; N];
    let mut hasher = Sha256::new();

    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }

        hasher.update(&buffer[..read]);
    }

    let computed = hasher.finalize();

    Ok(computed.as_slice() == chksum.0)
}

/// Splits a crate package name into its component parts
///
/// `<crate-name>-<semver>.crate`
///
/// The naming is a bit annoying for these since the separator between
/// the crate name and version is a `-` which can be present in both
/// the crate name as well as the semver, so we have to take those into account
#[inline]
pub fn crate_file_components(name: &str) -> Option<(&str, &str)> {
    let name = name.strip_suffix(".crate")?;

    // The first `.` should be after the major version
    let dot = name.find('.')?;
    let dash_sep = name[..dot].rfind('-')?;

    Some((&name[..dash_sep], &name[dash_sep + 1..]))
}

impl LocalRegistry {
    /// Validates the specified path contains a local registry
    ///
    /// Validation ensures every crate file matches the expected according to
    /// the index entry for the crate
    pub fn validate(index: &super::LocalIndex) -> Result<(), Error> {
        let rd = std::fs::read_dir(index.path.parent().unwrap())
            .map_err(|err| Error::IoPath(err, index.path.parent().unwrap().to_owned()))?;

        // Don't bother deserializing multiple times if there are multiple versions
        // of the same crate
        let mut indexed = std::collections::BTreeMap::new();

        for entry in rd {
            let Ok(entry) = entry else {
                continue;
            };
            if entry.file_type().map_or(true, |ft| !ft.is_file()) {
                continue;
            }
            let Ok(path) = PathBuf::from_path_buf(entry.path()) else {
                continue;
            };

            let Some(fname) = path.file_name() else {
                continue;
            };
            let Some((crate_name, version)) = crate_file_components(fname) else {
                continue;
            };

            let index_entry = if let Some(ie) = indexed.get(crate_name) {
                ie
            } else {
                let krate_name: crate::KrateName<'_> = crate_name.try_into()?;
                let path = index.krate_path(krate_name);

                let index_contents = match std::fs::read(&path) {
                    Ok(ic) => ic,
                    Err(err) => return Err(Error::IoPath(err, path)),
                };

                let ik = IndexKrate::from_slice(&index_contents)?;

                indexed.insert(crate_name.to_owned(), ik);
                indexed.get(crate_name).unwrap()
            };

            let index_vers = index_entry
                .versions
                .iter()
                .find(|kv| kv.version == version)
                .ok_or_else(|| LocalRegistryError::MissingVersion {
                    name: crate_name.to_owned(),
                    version: version.into(),
                })?;

            // Read the crate file from disk and verify its checksum matches the one stated in the index
            let file =
                std::fs::File::open(&path).map_err(|err| Error::IoPath(err, path.clone()))?;
            if !validate_checksum::<{ 8 * 1024 }>(&file, &index_vers.checksum)
                .map_err(|err| Error::IoPath(err, path.clone()))?
            {
                return Err(LocalRegistryError::ChecksumMismatch {
                    name: crate_name.to_owned(),
                    version: version.into(),
                }
                .into());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn gets_components() {
        use super::crate_file_components as cfc;

        assert_eq!(cfc("cc-1.0.75.crate").unwrap(), ("cc", "1.0.75"));
        assert_eq!(
            cfc("cfg-expr-0.1.0-dev+1234.crate").unwrap(),
            ("cfg-expr", "0.1.0-dev+1234")
        );
        assert_eq!(
            cfc("android_system_properties-0.1.5.crate").unwrap(),
            ("android_system_properties", "0.1.5")
        );
    }
}
