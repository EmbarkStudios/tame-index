#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Cache(#[from] CacheError),
    #[error("unable to use non-utf8 path {:?}", .0)]
    NonUtf8Path(std::path::PathBuf),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("I/O operation failed for path '{}'", .1)]
    IoPath(#[source] std::io::Error, crate::PathBuf),
    #[error("crate names are not allowed to be empty")]
    EmptyCrateName,
    #[error("crate names may only contain ASCII characters")]
    NonAsciiCrateName,
    #[error(transparent)]
    InvalidUrl(#[from] InvalidUrl),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Toml(#[from] toml::de::Error),
    #[error("index entry contained no versions for the crate")]
    NoCrateVersions,
    #[error(transparent)]
    Http(#[from] HttpError),
    #[cfg(feature = "git")]
    #[error(transparent)]
    Git(#[from] crate::index::git_remote::GitError),
    #[error(transparent)]
    Semver(#[from] semver::Error),
}

impl From<std::path::PathBuf> for Error {
    fn from(p: std::path::PathBuf) -> Self {
        Self::NonUtf8Path(p)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("the url '{url}' is invalid")]
pub struct InvalidUrl {
    /// The invalid url
    pub url: String,
    pub source: InvalidUrlError,
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidUrlError {
    #[error("sparse indices require the use of a url that starts with `sparse+http`")]
    MissingSparse,
    #[error("the scheme modifier is unknown")]
    UnknownSchemeModifier,
    #[error("the scheme is missing")]
    MissingScheme,
}

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("the cache entry was malformed")]
    InvalidCacheEntry,
    #[error("the cache entry is an old, unsupported version")]
    OutdatedCacheVersion,
    #[error("the cache entry is an unknown version, possibly written by a newer cargo version")]
    UnknownCacheVersion,
    #[error(
        "the cache entry's index version is unknown, possibly written by a newer cargo version"
    )]
    UnknownIndexVersion,
    #[error("the cache entry's revision does not match the current revision")]
    OutdatedRevision,
    #[error("a specific version in the cache entry is malformed")]
    InvalidCrateVersion,
}

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[cfg(feature = "sparse")]
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error("status code '{code}': {msg}")]
    StatusCode {
        code: http::StatusCode,
        msg: &'static str,
    },
    #[error(transparent)]
    Http(#[from] http::Error),
}
