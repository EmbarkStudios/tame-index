//! Provides functionality for interacting with both local and remote registry
//! indices

pub mod cache;
#[cfg(all(feature = "git", feature = "sparse"))]
mod combo;
#[allow(missing_docs)]
pub mod git;
#[cfg(feature = "git")]
pub(crate) mod git_remote;
pub mod location;
#[allow(missing_docs)]
pub mod sparse;
#[cfg(feature = "sparse")]
mod sparse_remote;

pub use cache::IndexCache;
#[cfg(all(feature = "git", feature = "sparse"))]
pub use combo::ComboIndex;
pub use git::GitIndex;
#[cfg(feature = "git")]
pub use git_remote::RemoteGitIndex;
pub use location::{IndexLocation, IndexPath, IndexUrl};
pub use sparse::SparseIndex;
#[cfg(feature = "sparse")]
pub use sparse_remote::{AsyncRemoteSparseIndex, RemoteSparseIndex};

/// Global configuration of an index, reflecting the [contents of config.json](https://doc.rust-lang.org/cargo/reference/registries.html#index-format).
#[derive(Eq, PartialEq, PartialOrd, Ord, Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct IndexConfig {
    /// Pattern for creating download URLs. See [`Self::download_url`].
    pub dl: String,
    /// Base URL for publishing, etc.
    pub api: Option<String>,
}

impl IndexConfig {
    /// Gets the download url for the specified crate version
    ///
    /// See <https://doc.rust-lang.org/cargo/reference/registries.html#index-format>
    /// for more info
    pub fn download_url(&self, name: crate::KrateName<'_>, version: &str) -> String {
        let mut dl = self.dl.clone();

        while let Some(start) = dl.find("{crate}") {
            dl.replace_range(start..start + 7, name.0);
        }

        while let Some(start) = dl.find("{version}") {
            dl.replace_range(start..start + 9, version);
        }

        if dl.contains("{prefix}") || dl.contains("{lowerprefix}") {
            let mut prefix = String::with_capacity(6);
            name.prefix(&mut prefix, '/');

            while let Some(start) = dl.find("{prefix}") {
                dl.replace_range(start..start + 8, &prefix);
            }

            if dl.contains("{lowerprefix}") {
                prefix.make_ascii_lowercase();

                while let Some(start) = dl.find("{lowerprefix}") {
                    dl.replace_range(start..start + 13, &prefix);
                }
            }
        }

        dl
    }
}

use crate::{Error, Path, PathBuf};

/// Provides simpler access to the cache for an index, regardless of the registry kind
pub enum ComboIndexCache {
    /// A git index
    Git(GitIndex),
    /// A sparse HTTP index
    Sparse(SparseIndex),
}

impl ComboIndexCache {
    /// Retrieves the index metadata for the specified crate name, optionally
    /// writing a cache entry for it if there was not already an up to date one
    #[inline]
    pub fn cached_krate(
        &self,
        name: crate::KrateName<'_>,
    ) -> Result<Option<crate::IndexKrate>, Error> {
        match self {
            Self::Git(index) => index.cached_krate(name),
            Self::Sparse(index) => index.cached_krate(name),
        }
    }

    /// Constructs a [`Self`] for the specified index.
    ///
    /// See [`Self::crates_io`] if you want to create a crates.io index based
    /// upon other information in the user's environment
    pub fn new(il: IndexLocation<'_>) -> Result<Self, Error> {
        let index = if il.url.is_sparse() {
            let sparse = SparseIndex::new(il)?;
            Self::Sparse(sparse)
        } else {
            let git = GitIndex::new(il)?;
            Self::Git(git)
        };

        Ok(index)
    }

    /// Opens the default index for crates.io, depending on the configuration and
    /// version of cargo
    ///
    /// 1. Determines if the crates.io registry has been replaced
    /// 2. Determines the protocol explicitly configured by the user
    /// <https://doc.rust-lang.org/cargo/reference/config.html#registriescrates-ioprotocol>
    /// 3. If not specified, detects the version of cargo (see
    /// [`Self::cargo_version`]), and uses that to determine the appropriate default
    pub fn crates_io(
        config_root: Option<PathBuf>,
        cargo_home: Option<PathBuf>,
        cargo_version: Option<&str>,
    ) -> Result<Self, Error> {
        // If the crates.io registry has been replaced it doesn't matter what
        // the protocol for it has been changed to
        if let Some(replacement) =
            get_crates_io_replacement(config_root.clone(), cargo_home.as_deref())?
        {
            let il = IndexLocation::new(IndexUrl::NonCratesIo(&replacement)).with_root(cargo_home);
            return Self::new(il);
        }

        let sparse_index = match std::env::var("CARGO_REGISTRIES_CRATES_IO_PROTOCOL")
            .ok()
            .as_deref()
        {
            Some("sparse") => true,
            Some("git") => false,
            _ => {
                let sparse_index =
                    read_cargo_config(config_root, cargo_home.as_deref(), |config| {
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

                if let Some(si) = sparse_index {
                    si
                } else {
                    let semver = match cargo_version {
                        Some(v) => std::borrow::Cow::Borrowed(v),
                        None => Self::cargo_version()?.into(),
                    };

                    let vers: semver::Version = semver.parse()?;
                    vers >= semver::Version::new(1, 70, 0)
                }
            }
        };

        Self::new(
            IndexLocation::new(if sparse_index {
                IndexUrl::CratesIoSparse
            } else {
                IndexUrl::CratesIoGit
            })
            .with_root(cargo_home),
        )
    }

    /// Retrieves the current version of cargo being used
    pub fn cargo_version() -> Result<String, Error> {
        use std::io;

        let mut cargo = std::process::Command::new(
            std::env::var_os("CARGO")
                .as_deref()
                .unwrap_or(std::ffi::OsStr::new("cargo")),
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
}

impl From<SparseIndex> for ComboIndexCache {
    #[inline]
    fn from(si: SparseIndex) -> Self {
        Self::Sparse(si)
    }
}

impl From<GitIndex> for ComboIndexCache {
    #[inline]
    fn from(gi: GitIndex) -> Self {
        Self::Git(gi)
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
                let contents = match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(err) => return Err(Error::IoPath(err, path)),
                };

                let toml: toml::Value = toml::from_str(&contents)?;
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
    use super::ComboIndexCache;

    // Current stable is 1.70.0, both these tests should pass

    #[test]
    fn gets_cargo_version() {
        const MINIMUM: semver::Version = semver::Version::new(1, 70, 0);
        let version: semver::Version = ComboIndexCache::cargo_version().unwrap().parse().unwrap();
        assert!(version >= MINIMUM);
    }

    #[test]
    fn opens_sparse() {
        assert!(std::env::var_os("CARGO_REGISTRIES_CRATES_IO_PROTOCOL").is_none());
        assert!(matches!(
            ComboIndexCache::crates_io(None, None, None).unwrap(),
            ComboIndexCache::Sparse(_)
        ));
    }
}
