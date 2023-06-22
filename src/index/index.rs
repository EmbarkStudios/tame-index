use crate::{Error, IndexKrate, Path, PathBuf, RemoteGitIndex, RemoteSparseIndex};

/// A wrapper around either a [`RemoteGitIndex`] or [`RemoteSparseIndex`]
pub enum Index {
    /// A standard git based registry index. No longer the default for crates.io
    /// as of 1.70.0
    Git(RemoteGitIndex),
    /// An HTTP sparse index
    Sparse(RemoteSparseIndex),
}

impl Index {
    /// Opens the default index for crates.io, depending on the configuration and
    /// version of cargo
    ///
    /// 1. Determines if the crates.io registry has been replaced
    /// 2. Determines the protocol explicitly configured by the user
    /// <https://doc.rust-lang.org/cargo/reference/config.html#registriescrates-ioprotocol>
    /// 3. If not specified, detects the version of cargo (see
    /// [`Self::cargo_version`]), and uses that to determine the appropriate default
    pub fn crates_io(
        root: Option<&Path>,
        cargo_home: Option<&Path>,
        cargo_version: Option<&str>,
    ) -> Result<Self, Error> {
        // If the crates.io registry has been replaced it doesn't matter what
        // the protocol for it has been changed to
        if let Some(replacement) = get_crates_io_replacement(root, cargo_home)? {
            let (path, canonical) = utils::get_index_details(&replacement, cargo_home)?;

            return if canonical.starts_with("sparse+http") {
                Ok(Self::Sparse(SparseIndex::at_path(path, canonical)))
            } else {
                Index::with_path(path, canonical).map(Self::Git)
            };
        }

        let sparse_index = match std::env::var("CARGO_REGISTRIES_CRATES_IO_PROTOCOL")
            .ok()
            .as_deref()
        {
            Some("sparse") => true,
            Some("git") => false,
            _ => {
                let sparse_index = config::read_cargo_config(root, cargo_home, |config| {
                    config
                        .get("registries")
                        .and_then(|v| v.get("crates-io"))
                        .and_then(|v| v.get("protocol"))
                        .and_then(|v| v.as_str())
                        .and_then(|v| match v {
                            "sparse" => Some(true),
                            "git" => Some(false),
                            _ => None,
                        })
                })?;

                match sparse_index {
                    Some(si) => si,
                    None => {
                        let semver = match cargo_version {
                            Some(v) => std::borrow::Cow::Borrowed(v),
                            None => Self::cargo_version()?.into(),
                        };

                        // Note this would need to change if there was ever a major version
                        // bump of cargo, but that's unlikely (famous last words)
                        let minor = semver.split('.').nth(1).ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "cargo semver was in an invalid format",
                            )
                        })?;

                        let minor: u32 = minor
                            .parse()
                            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

                        minor >= 70
                    }
                }
            }
        };

        let url = if sparse_index {
            crate::CRATES_IO_HTTP_INDEX
        } else {
            crate::INDEX_GIT_URL
        };

        let (path, canonical) = dirs::get_index_details(url, cargo_home)?;

        if sparse_index {
            Ok(Self::Sparse(SparseIndex::at_path(path, canonical)))
        } else {
            Index::with_path(path, canonical).map(Self::Git)
        }
    }

    /// Retrieves the current version of cargo being used
    pub fn cargo_version() -> Result<String, Error> {
        let mut cargo = std::process::Command::new(
            std::env::var_os("CARGO")
                .as_deref()
                .unwrap_or_else(|| std::ffi::OsStr::new("cargo")),
        );

        cargo.arg("-V");
        cargo.stdout(std::process::Stdio::piped());

        let output = cargo.output()?;
        if !output.status.success() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "failed to request cargo version information",
            )
            .into());
        }

        let stdout = String::from_utf8(output.stdout)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

        let semver = stdout.split(' ').nth(1).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "cargo version information was in an invalid format",
            )
        })?;

        Ok(semver.to_owned())
    }

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

    /// Retrieves the index metadata for the specified crate name, optionally
    /// writing a cache entry for it if there was not already an up to date one
    #[inline]
    pub fn cached_krate(&self, name: KrateName<'_>) -> Result<Option<IndexKrate>, Error> {
        match self {
            Self::Git(index) => index.krate(name, write_cache_entry),
            Self::Sparse(index) => index.krate(name, write_cache_entry),
        }
    }
}

impl From<RemoteGitIndex> for RegistryIndex {
    #[inline]
    fn from(index: RemoteGitIndex) -> Self {
        Self::Git(index)
    }
}

impl From<RemoteSparseIndex> for RegistryIndex {
    #[inline]
    fn from(index: RemoteSparseIndex) -> Self {
        Self::Sparse(index)
    }
}

/// Calls the specified function for each cargo config located according to
/// cargo's standard hierarchical structure
///
/// Note that this only supports the use of `.cargo/config.toml`, which is not
/// supported below cargo 1.39.0
///
/// See <https://doc.rust-lang.org/cargo/reference/config.html#hierarchical-structure>
pub(crate) fn read_cargo_config<T>(
    root: Option<PathBuf>,
    cargo_home: Option<&Path>,
    callback: impl Fn(&toml::Value) -> Option<T>,
) -> Result<Option<T>, Error> {
    use std::borrow::Cow;

    if let Some(mut path) = root.or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|pb| PathBuf::from_path_buf(pb).ok())
    }) {
        loop {
            path.push(".cargo/config.toml");
            if path.exists() {
                let toml: toml::Value = toml::from_str(&std::fs::read_to_string(&path)?)?;
                if let Some(value) = callback(&toml) {
                    return Ok(Some(value));
                }
            }
            path.pop();
            path.pop();

            // Walk up to the next potential config root
            if !path.pop() {
                break;
            }
        }
    }

    if let Some(home) = cargo_home
        .map(Cow::Borrowed)
        .or_else(|| crate::utils::cargo_home().ok().map(Cow::Owned))
    {
        let path = home.join("config.toml");
        if path.exists() {
            let toml: toml::Value =
                toml::from_str(&std::fs::read_to_string(&path)?).map_err(Error::Toml)?;
            if let Some(value) = callback(&toml) {
                return Ok(Some(value));
            }
        }
    }

    Ok(None)
}

/// Gets the url of a replacement registry for crates.io if one has been configured
///
/// See <https://doc.rust-lang.org/cargo/reference/source-replacement.html>
#[inline]
pub(crate) fn get_crates_io_replacement(
    root: Option<PathBuf>,
    cargo_home: Option<&Path>,
) -> Result<Option<String>, Error> {
    read_cargo_config(root, cargo_home, |config| {
        config.get("source").and_then(|sources| {
            sources
                .get("crates-io")
                .and_then(|v| v.get("replace-with"))
                .and_then(|v| v.as_str())
                .and_then(|v| sources.get(v))
                .and_then(|v| v.get("registry"))
                .and_then(|v| v.as_str().map(String::from))
        })
    })
}

#[cfg(test)]
mod test {
    use crate::RegistryIndex;

    // Current stable is 1.70.0, both these tests should pass

    #[test]
    fn gets_cargo_version() {
        const MINIMUM: semver::Version = semver::Version::new(1, 70, 0);
        assert!(RegistryIndex::cargo_version().unwrap().parse().unwrap() >= MINIMUM);
    }

    #[test]
    fn opens_sparse() {
        assert!(std::env::var_os("CARGO_REGISTRIES_CRATES_IO_PROTOCOL").is_none());
        assert!(matches!(
            RegistryIndex::crates_io(None, None, None).unwrap(),
            RegistryIndex::Sparse(_)
        ));
    }
}
