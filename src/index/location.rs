//! Helpers for initializing the remote and local disk location of an index

use crate::{Error, Path, PathBuf};
use std::borrow::Cow;

/// A remote index url
#[derive(Default, Debug)]
pub enum IndexUrl<'iu> {
    /// The canonical crates.io HTTP sparse index.
    ///
    /// See [`crate::CRATES_IO_HTTP_INDEX`]
    #[default]
    CratesIoSparse,
    /// The canonical crates.io git index.
    ///
    /// See [`crate::CRATES_IO_INDEX`]
    CratesIoGit,
    /// A non-crates.io index.
    ///
    /// This variant uses the url to determine the index kind (sparse or git) by
    /// inspecting the url's scheme. This is because sparse indices are required
    /// to have the `sparse+` scheme modifier
    NonCratesIo(Cow<'iu, str>),
    /// A [local registry](crate::index::LocalRegistry)
    Local(Cow<'iu, Path>),
}

impl<'iu> IndexUrl<'iu> {
    /// Gets the url as a string
    pub fn as_str(&'iu self) -> &'iu str {
        match self {
            Self::CratesIoSparse => crate::CRATES_IO_HTTP_INDEX,
            Self::CratesIoGit => crate::CRATES_IO_INDEX,
            Self::NonCratesIo(url) => url,
            Self::Local(pb) => pb.as_str(),
        }
    }

    /// Returns true if the url points to a sparse registry
    pub fn is_sparse(&self) -> bool {
        match self {
            Self::CratesIoSparse => true,
            Self::CratesIoGit | Self::Local(..) => false,
            Self::NonCratesIo(url) => url.starts_with("sparse+http"),
        }
    }

    /// Gets the [`IndexUrl`] for crates.io, depending on the local environment.
    ///
    /// 1. Determines if the crates.io registry has been [replaced](https://doc.rust-lang.org/cargo/reference/source-replacement.html)
    /// 2. Determines if the protocol was explicitly [configured](https://doc.rust-lang.org/cargo/reference/config.html#registriescrates-ioprotocol) by the user
    /// 3. Otherwise, detects the version of cargo (see [`crate::utils::cargo_version`]), and uses that to determine the appropriate default
    pub fn crates_io(
        config_root: Option<PathBuf>,
        cargo_home: Option<&Path>,
        cargo_version: Option<&str>,
    ) -> Result<Self, Error> {
        // If the crates.io registry has been replaced it doesn't matter what
        // the protocol for it has been changed to
        if let Some(replacement) = get_crates_io_replacement(config_root.clone(), cargo_home)? {
            return Ok(replacement);
        }

        let sparse_index = match std::env::var("CARGO_REGISTRIES_CRATES_IO_PROTOCOL")
            .ok()
            .as_deref()
        {
            Some("sparse") => true,
            Some("git") => false,
            _ => {
                let sparse_index = read_cargo_config(config_root, cargo_home, |config| {
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
                        None => crate::utils::cargo_version(None)?.into(),
                    };

                    let vers: semver::Version = semver.parse()?;
                    vers >= semver::Version::new(1, 70, 0)
                }
            }
        };

        Ok(if sparse_index {
            Self::CratesIoSparse
        } else {
            Self::CratesIoGit
        })
    }
}

impl<'iu> From<&'iu str> for IndexUrl<'iu> {
    #[inline]
    fn from(s: &'iu str) -> Self {
        Self::NonCratesIo(s.into())
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
pub(crate) fn get_crates_io_replacement<'iu>(
    root: Option<PathBuf>,
    cargo_home: Option<&Path>,
) -> Result<Option<IndexUrl<'iu>>, Error> {
    read_cargo_config(root, cargo_home, |config| {
        config.get("source").and_then(|sources| {
            sources
                .get("crates-io")
                .and_then(|v| v.get("replace-with"))
                .and_then(|v| v.as_str())
                .and_then(|v| sources.get(v))
                .and_then(|v| {
                    v.get("registry")
                        .and_then(|reg| {
                            reg.as_str()
                                .map(|r| IndexUrl::NonCratesIo(r.to_owned().into()))
                        })
                        .or_else(|| {
                            v.get("local-registry").and_then(|l| {
                                l.as_str().map(|l| IndexUrl::Local(PathBuf::from(l).into()))
                            })
                        })
                })
        })
    })
}

#[cfg(test)]
mod test {
    // Current stable is 1.70.0
    #[test]
    fn opens_sparse() {
        assert!(std::env::var_os("CARGO_REGISTRIES_CRATES_IO_PROTOCOL").is_none());
        assert!(matches!(
            crate::index::ComboIndexCache::new(super::IndexLocation::new(
                super::IndexUrl::crates_io(None, None, None).unwrap()
            ))
            .unwrap(),
            crate::index::ComboIndexCache::Sparse(_)
        ));
    }
}
