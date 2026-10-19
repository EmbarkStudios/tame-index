//! Provides the various error types for this crate

use std::{error::Error as StdErr, fmt};

#[cfg(feature = "local-builder")]
pub use crate::index::local::builder::LocalRegistryError;

/// The core error type for this library
#[derive(Debug)]
pub enum Error {
    /// Failed to deserialize a local cache entry
    Cache(CacheError),
    /// This library assumes utf-8 paths in all cases, a path was provided that
    /// was not valid utf-8
    NonUtf8Path(std::path::PathBuf),
    /// An environment variable was located, but had a non-utf8 value
    NonUtf8EnvVar(std::borrow::Cow<'static, str>),
    /// A user-provided string was not a valid crate name
    InvalidKrateName(InvalidKrateName),
    /// The user specified a registry name that did not exist in any searched
    /// .cargo/config.toml
    UnknownRegistry(String),
    /// An I/O error
    Io(std::io::Error),
    /// An I/O error occurred trying to access a specific path
    IoPath(std::io::Error, crate::PathBuf),
    /// A user provided URL was invalid
    InvalidUrl(InvalidUrl),
    /// Failed to de/serialize JSON
    Json(serde_json::Error),
    /// Failed to deserialize TOML
    Toml(Box<toml_span::Error>),
    /// An index entry did not contain any versions
    NoCrateVersions,
    /// Failed to handle an HTTP response or request
    Http(HttpError),
    /// Failed to parse a semver version or requirement
    Semver(semver::Error),
    /// Failed to lock a file
    Lock(FileLockError),
    /// The entry from a remote index was invalid
    InvalidJsonLines(InvalidJsonLines),
    /// A local registry is invalid
    #[cfg(feature = "local-builder")]
    Local(LocalRegistryError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cache(_) => f.write_str("cache error"),
            Self::NonUtf8Path(p) => write!(f, "unable to use non-utf8 path {p:?}"),
            Self::NonUtf8EnvVar(k) => write!(f, "environment variable '{k}' has a non-utf8 value"),
            Self::InvalidKrateName(_) => f.write_str("invalid crate name"),
            Self::UnknownRegistry(name) => write!(
                f,
                "registry '{name}' was not located in any .cargo/config.toml"
            ),
            Self::Io(_) | Self::IoPath(_, _) => f.write_str("I/O error"),
            Self::InvalidUrl(_) => f.write_str("invalid url"),
            Self::Json(_) => f.write_str("invalid JSON"),
            Self::Toml(_) => f.write_str("invalid TOML"),
            Self::NoCrateVersions => f.write_str("index entry contained no versions"),
            Self::Http(_) => f.write_str("HTTP error"),
            Self::Semver(_) => f.write_str("invalid semver"),
            Self::Lock(_) => f.write_str("file lock error"),
            Self::InvalidJsonLines(_) => f.write_str("invalid JSON lines"),
            #[cfg(feature = "local-builder")]
            Self::Local(_) => f.write_str("local registry error"),
        }
    }
}

impl StdErr for Error {
    fn source(&self) -> Option<&(dyn StdErr + 'static)> {
        match self {
            Self::Cache(c) => Some(c),
            Self::InvalidKrateName(ke) => Some(ke),
            Self::Io(io) => Some(io),
            Self::IoPath(io, _) => Some(io),
            Self::InvalidUrl(iu) => Some(iu),
            Self::Json(je) => Some(je),
            Self::Toml(te) => Some(te),
            Self::Http(he) => Some(he),
            Self::Semver(se) => Some(se),
            Self::Lock(le) => Some(le),
            Self::InvalidJsonLines(ij) => Some(ij),
            #[cfg(feature = "local-builder")]
            Self::Local(lre) => Some(lre),
            _ => None,
        }
    }
}

macro_rules! from {
    ($k:ty, [$($v:ty => $m:ident),+$(,)?]) => {
        $(
            impl From<$v> for $k {
                fn from(v: $v) -> Self {
                    Self::$m(v)
                }
            }
        )+
    }
}

from!(Error, [
    CacheError => Cache,
    InvalidKrateName => InvalidKrateName,
    std::path::PathBuf => NonUtf8Path,
    std::io::Error => Io,
    InvalidUrl => InvalidUrl,
    serde_json::Error => Json,
    HttpError => Http,
    semver::Error => Semver,
    FileLockError => Lock,
    InvalidJsonLines => InvalidJsonLines,
]);

#[cfg(feature = "local-builder")]
impl From<LocalRegistryError> for Error {
    fn from(v: LocalRegistryError) -> Self {
        Self::Local(v)
    }
}

/// Various kinds of reserved names disallowed by cargo
#[derive(Debug, Copy, Clone)]
pub enum ReservedNameKind {
    /// The name is a Rust keyword
    Keyword,
    /// The name conflicts with a cargo artifact directory
    Artifact,
    /// The name has a special meaning on Windows
    Windows,
    /// The name conflicts with a Rust std library name
    Standard,
}

impl fmt::Display for ReservedNameKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Keyword => f.write_str("rustlang keyword"),
            Self::Artifact => f.write_str("cargo artifact"),
            Self::Windows => f.write_str("windows reserved"),
            Self::Standard => f.write_str("rustlang std library"),
        }
    }
}

/// Errors that can occur when validating a crate name
#[derive(Debug)]
pub enum InvalidKrateName {
    /// The name had an invalid length
    InvalidLength(usize),
    /// The name contained an invalid character
    InvalidCharacter {
        /// The invalid character
        invalid: char,
        /// The index of the character in the provided string
        index: usize,
    },
    /// The name was one of the reserved names disallowed by cargo
    ReservedName {
        /// The name that was reserved
        reserved: &'static str,
        /// The kind of the reserved name
        kind: ReservedNameKind,
    },
}

impl fmt::Display for InvalidKrateName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength(len) => write!(f, "crate name had an invalid length of '{len}'"),
            Self::InvalidCharacter { invalid, index } => {
                write!(f, "invalid character '{invalid}` @ {index}")
            }
            Self::ReservedName { reserved, kind } => {
                write!(f, "the name '{reserved}' is reserved as '{kind}`")
            }
        }
    }
}

impl StdErr for InvalidKrateName {
    fn source(&self) -> Option<&(dyn StdErr + 'static)> {
        None
    }
}

/// An error pertaining to a bad URL provided to the API
#[derive(Debug)]
pub struct InvalidUrl {
    /// The invalid url
    pub url: String,
    /// The reason it is invalid
    pub source: InvalidUrlError,
}

impl fmt::Display for InvalidUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the url '{}' is invalid", self.url)
    }
}

impl StdErr for InvalidUrl {
    fn source(&self) -> Option<&(dyn StdErr + 'static)> {
        Some(&self.source)
    }
}

/// The specific reason for the why the URL is invalid
#[derive(Debug)]
pub enum InvalidUrlError {
    /// Sparse HTTP registry urls must be of the form `sparse+http(s)://`
    MissingSparse,
    /// The `<modifier>+<scheme>://` is not supported
    UnknownSchemeModifier,
    /// Unable to find the `<scheme>://`
    MissingScheme,
    /// Attempted to construct a git index with a sparse URL
    SparseForGit,
}

impl fmt::Display for InvalidUrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSparse => f.write_str(
                "sparse indices require the use of a url that starts with `sparse+http`",
            ),
            Self::UnknownSchemeModifier => f.write_str("the scheme modifier is unknown"),
            Self::MissingScheme => f.write_str("the scheme is missing"),
            Self::SparseForGit => f.write_str("attempted to create a git index for a sparse URL"),
        }
    }
}

impl StdErr for InvalidUrlError {
    fn source(&self) -> Option<&(dyn StdErr + 'static)> {
        None
    }
}

/// Errors related to a local index cache
#[derive(Debug)]
pub enum CacheError {
    /// The cache entry is malformed
    InvalidCacheEntry,
    /// The cache version is old
    OutdatedCacheVersion,
    /// The cache version is newer than the version supported by this crate
    UnknownCacheVersion,
    /// The index version is newer than the version supported by this crate
    UnknownIndexVersion,
    /// The revision in the cache entry did match the requested revision
    ///
    /// This can occur when a git index is fetched and a newer revision is pulled
    /// from the remote index, invalidating all local cache entries6
    OutdatedRevision,
    /// A crate version in the cache file was malformed
    InvalidCrateVersion,
}

impl fmt::Display for CacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCacheEntry => f.write_str("the cache entry is malformed"),
            Self::OutdatedCacheVersion => f.write_str("the cache entry is an old, unsupported version"),
            Self::UnknownCacheVersion => f.write_str("the cache entry is an unknown version, possibly written by a newer cargo version"),
            Self::UnknownIndexVersion => f.write_str("the cache entry's index version is unknown, possibly written by a newer cargo version"),
            Self::OutdatedRevision => f.write_str("the cache entry's revision does not match the current revision"),
            Self::InvalidCrateVersion => f.write_str("a specific version in the cache entry is malformed"),
        }
    }
}

impl StdErr for CacheError {
    fn source(&self) -> Option<&(dyn StdErr + 'static)> {
        None
    }
}

/// Errors related to HTTP requests or responses
#[derive(Debug)]
pub enum HttpError {
    /// A status code was received that indicates user error, or possibly a
    /// remote index that does not follow the protocol supported by this crate
    StatusCode {
        /// The status code
        code: http::StatusCode,
        /// The reason the status code raised an error
        msg: &'static str,
    },
    /// A [`http::Error`]
    Http(http::Error),
    /// A string could not be parsed as a valid header value
    InvalidHeaderValue(http::header::InvalidHeaderValue),
}

from!(HttpError, [
    http::Error => Http,
    http::header::InvalidHeaderValue => InvalidHeaderValue,
]);

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StatusCode { code, msg } => write!(f, "status code '{code}': {msg}"),
            Self::Http(_) => f.write_str("`http` error"),
            Self::InvalidHeaderValue(_) => f.write_str("invalid header value"),
        }
    }
}

impl StdErr for HttpError {
    fn source(&self) -> Option<&(dyn StdErr + 'static)> {
        match self {
            Self::Http(he) => Some(he),
            Self::InvalidHeaderValue(ihv) => Some(ihv),
            Self::StatusCode { .. } => None,
        }
    }
}

/// Errors when validating a remote index entry
#[derive(Debug)]
pub enum InvalidJsonLines {
    /// The entry for a crate was empty, every entry must have at least one published version to be valid
    Empty,
    /// A specific line did not contain a JSON object
    NotJson(u32),
}

impl fmt::Display for InvalidJsonLines {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("the entry was empty"),
            Self::NotJson(i) => write!(f, "the entry had invalid JSON on line {i}"),
        }
    }
}

impl StdErr for InvalidJsonLines {
    fn source(&self) -> Option<&(dyn StdErr + 'static)> {
        None
    }
}

/// An error pertaining to a failed file lock
#[derive(Debug)]
pub struct FileLockError {
    /// The path of the file lock
    pub path: crate::PathBuf,
    /// The underlying failure reason
    pub source: LockError,
}

impl fmt::Display for FileLockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to obtain lock file '{}'", self.path)
    }
}

impl StdErr for FileLockError {
    fn source(&self) -> Option<&(dyn StdErr + 'static)> {
        Some(&self.source)
    }
}

/// Errors that can occur when attempting to acquire a [`FileLock`]
#[derive(Debug)]
pub enum LockError {
    /// An I/O error occurred attempting to open the lock file
    Open(std::io::Error),
    /// Exclusive locks cannot be take on read-only file systems
    Readonly,
    /// Failed to create parents directories to lock file
    CreateDir(std::io::Error),
    /// Locking is not supported if the lock file is on an NFS, though note this
    /// is a bit more nuanced as `NFSv4` _does_ support file locking, but is out
    /// of scope, at least for now
    Nfs,
    /// This could happen on eg. _extremely_ old and outdated OSes or some filesystems
    /// and is only present for completeness
    NotSupported,
    /// An I/O error occurred attempting to un/lock the file
    Lock(std::io::Error),
    /// The lock could not be acquired within the caller provided timeout
    TimedOut,
    /// The lock is currently held by another process
    Contested,
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(io) => write!(f, "{io}"),
            Self::Readonly => {
                f.write_str("attempted to take an exclusive lock on a read-only path")
            }
            Self::CreateDir(io) => {
                write!(f, "failed to create parent directories for lock path: {io}")
            }
            Self::Nfs => f.write_str("NFS do not support locking"),
            Self::NotSupported => {
                f.write_str("locking is not supported on the filesystem and/or in the kernel")
            }
            Self::Lock(io) => write!(f, "failed to acquire or release file lock: {io}"),
            Self::TimedOut => f.write_str("failed to acquire lock within the specified duration"),
            Self::Contested => f.write_str("the lock is currently held by another process"),
        }
    }
}

impl StdErr for LockError {
    fn source(&self) -> Option<&(dyn StdErr + 'static)> {
        None
    }
}
