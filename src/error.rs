//! Provides the various error types for this crate

#[cfg(feature = "git")]
pub use crate::index::git_remote::GitError;

/// The core error type for this library
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Failed to deserialize a local cache entry
    #[error(transparent)]
    Cache(#[from] CacheError),
    /// This library assumes utf-8 paths in all cases, a path was provided that
    /// was not valid utf-8
    #[error("unable to use non-utf8 path {:?}", .0)]
    NonUtf8Path(std::path::PathBuf),
    /// An I/O error
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// An I/O error occurred trying to access a specific path
    #[error("I/O operation failed for path '{}'", .1)]
    IoPath(#[source] std::io::Error, crate::PathBuf),
    /// Crate names must not be empty
    #[error("crate names are not allowed to be empty")]
    EmptyCrateName,
    /// Crates names must be ASCII.
    ///
    /// Note this is a rule enforced by crates.io, but not cargo itself. Please
    /// file an issue if you use a non-crates.io registry that does allow such
    /// non-ASCII crate names
    #[error("crate names may only contain ASCII characters")]
    NonAsciiCrateName,
    /// A user provided URL was invalid
    #[error(transparent)]
    InvalidUrl(#[from] InvalidUrl),
    /// Failed to de/serialize JSON
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Failed to deserialize TOML
    #[error(transparent)]
    Toml(#[from] toml::de::Error),
    /// An index entry did not contain any versions
    #[error("index entry contained no versions for the crate")]
    NoCrateVersions,
    /// Failed to handle an HTTP response or request
    #[error(transparent)]
    Http(#[from] HttpError),
    /// An error occurred doing a git operation
    #[cfg(feature = "git")]
    #[error(transparent)]
    Git(#[from] GitError),
    /// Failed to parse a semver version or requirement
    #[error(transparent)]
    Semver(#[from] semver::Error),
}

impl From<std::path::PathBuf> for Error {
    fn from(p: std::path::PathBuf) -> Self {
        Self::NonUtf8Path(p)
    }
}

/// An error pertaining to a bad URL provided to the API
#[derive(Debug, thiserror::Error)]
#[error("the url '{url}' is invalid")]
pub struct InvalidUrl {
    /// The invalid url
    pub url: String,
    /// The reason it is invalid
    pub source: InvalidUrlError,
}

/// The specific reason for the why the URL is invalid
#[derive(Debug, thiserror::Error)]
pub enum InvalidUrlError {
    /// Sparse HTTP registry urls must be of the form `sparse+http(s)://`
    #[error("sparse indices require the use of a url that starts with `sparse+http`")]
    MissingSparse,
    /// The `<modifier>+<scheme>://` is not supported
    #[error("the scheme modifier is unknown")]
    UnknownSchemeModifier,
    /// Unable to find the `<scheme>://`
    #[error("the scheme is missing")]
    MissingScheme,
}

/// Errors related to a local index cache
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// The cache entry is malformed
    #[error("the cache entry is malformed")]
    InvalidCacheEntry,
    /// The cache version is old
    #[error("the cache entry is an old, unsupported version")]
    OutdatedCacheVersion,
    /// The cache version is newer than the version supported by this crate
    #[error("the cache entry is an unknown version, possibly written by a newer cargo version")]
    UnknownCacheVersion,
    /// The index version is newer than the version supported by this crate
    #[error(
        "the cache entry's index version is unknown, possibly written by a newer cargo version"
    )]
    UnknownIndexVersion,
    /// The revision in the cache entry did match the requested revision
    ///
    /// This can occur when a git index is fetched and a newer revision is pulled
    /// from the remote index, invalidating all local cache entries
    #[error("the cache entry's revision does not match the current revision")]
    OutdatedRevision,
    /// A crate version in the cache file was malformed
    #[error("a specific version in the cache entry is malformed")]
    InvalidCrateVersion,
}

/// Errors related to HTTP requests or responses
#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    /// A [`reqwest::Error`]
    #[cfg(feature = "sparse")]
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    /// A status code was received that indicates user error, or possibly a
    /// remote index that does not follow the protocol supported by this crate
    #[error("status code '{code}': {msg}")]
    StatusCode {
        /// The status code
        code: http::StatusCode,
        /// The reason the status code raised an error
        msg: &'static str,
    },
    /// A [`http::Error`]
    #[error(transparent)]
    Http(#[from] http::Error),
}
