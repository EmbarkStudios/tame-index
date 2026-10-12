use super::{FileLock, IndexCache, cache::ValidCacheEntry};
use crate::{Error, HttpError, IndexKrate, KrateName};
use http::header::{self, HeaderName as Name, HeaderValue as Value};

/// The default URL of the crates.io HTTP index
pub const CRATES_IO_HTTP_INDEX: &str = "sparse+https://index.crates.io/";

pub enum HttpRevision<'stack> {
    /// The contents of the [`etag`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/ETag) header
    Etag(&'stack str),
    /// The contents of the [`last-modified`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Last-Modified) header
    LastModified(&'stack str),
}

/// A request to a remote index for the metadata entry for a specific crate
pub struct Request {
    pub uri: String,
    pub method: http::Method,
    pub headers: [(Name, Value); 3],
}

impl From<Request> for http::Request<()> {
    fn from(value: Request) -> Self {
        let mut r = http::Request::get(value.uri);

        let hm = r.headers_mut().unwrap();

        for (hn, hv) in value.headers {
            hm.insert(hn, hv);
        }

        r.body(()).unwrap()
    }
}

pub struct Response<'r> {
    pub status_code: http::StatusCode,
    pub revision: Option<HttpRevision<'r>>,
    pub body: &'r [u8],
}

impl<'r, B> From<&'r http::Response<B>> for Response<'r>
where
    B: AsRef<[u8]>,
{
    fn from(value: &'r http::Response<B>) -> Self {
        let hdrs = value.headers();

        let revision = if let Some(etag) = hdrs.get(header::ETAG).and_then(|v| v.to_str().ok()) {
            Some(HttpRevision::Etag(etag))
        } else if let Some(lm) = hdrs
            .get(header::LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
        {
            Some(HttpRevision::LastModified(lm))
        } else {
            None
        };

        Self {
            status_code: value.status(),
            revision,
            body: value.body().as_ref(),
        }
    }
}

/// Wrapper around managing a sparse HTTP index, re-using Cargo's local disk caches.
///
/// This implementation does no network I/O at all. If you want to make requests
/// to the remote index you may use the [`Self::make_remote_request`] and
/// [`Self::parse_remote_response`] methods, or you can enable the `sparse` feature
/// and and use [`RemoteSparseIndex`](crate::index::RemoteSparseIndex) or
/// [`AsyncRemoteSparseIndex`](crate::index::AsyncRemoteSparseIndex)
pub struct SparseIndex {
    cache: IndexCache,
    url: String,
}

impl SparseIndex {
    /// Creates a new sparse index for the specified location
    #[inline]
    pub fn new(il: crate::index::IndexLocation<'_>) -> Result<Self, Error> {
        if !il.url.is_sparse() {
            return Err(crate::InvalidUrl {
                url: il.url.as_str().to_owned(),
                source: crate::InvalidUrlError::MissingSparse,
            }
            .into());
        }

        let (path, url) = il.into_parts()?;
        Ok(Self {
            cache: IndexCache::at_path(path),
            url,
        })
    }

    /// Get the configuration of the index.
    ///
    /// See the [cargo docs](https://doc.rust-lang.org/cargo/reference/registry-index.html#index-configuration)
    pub fn index_config(&self) -> Result<super::IndexConfig, Error> {
        let path = self.cache.path.join("config.json");
        let bytes = std::fs::read(&path).map_err(|err| Error::IoPath(err, path))?;

        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Get the URL that can be used to fetch the index entry for the specified
    /// crate
    ///
    /// The body of a successful response for the returned URL can be parsed
    /// via [`IndexKrate::from_slice`]
    ///
    /// See [`Self::make_remote_request`] for a way to make a complete request
    #[inline]
    pub fn crate_url(&self, name: KrateName<'_>) -> String {
        let rel_path = name.relative_path(Some('/'));
        format!("{}{rel_path}", self.url())
    }

    /// The HTTP url of the index
    #[inline]
    pub fn url(&self) -> &str {
        self.url.strip_prefix("sparse+").unwrap_or(&self.url)
    }

    /// Gets the accessor to the local index cache
    #[inline]
    pub fn cache(&self) -> &IndexCache {
        &self.cache
    }

    /// Attempts to read the locally cached crate information
    #[inline]
    pub fn cached_krate(
        &self,
        name: KrateName<'_>,
        lock: &FileLock,
    ) -> Result<Option<IndexKrate>, Error> {
        self.cache.cached_krate(name, None, lock)
    }

    /// Creates an HTTP request that can be sent via your HTTP client of choice
    /// to retrieve the current metadata for the specified crate
    ///
    /// If specified, the revision is used instead of the possible revision stored in
    /// a local cache entry, resulting in no disk I/O being performed by this
    /// method
    ///
    /// See [`Self::parse_remote_response`] processing the response from the remote
    /// index
    ///
    /// It is highly recommended to assume HTTP/2 when making requests to remote
    /// indices, at least crates.io
    pub fn make_remote_request(
        &self,
        name: KrateName<'_>,
        revision: Option<HttpRevision<'_>>,
        lock: &FileLock,
    ) -> Result<Request, Error> {
        let uri = self.crate_url(name);

        // AFAICT this does not affect responses at the moment, but could in
        // the future if there are changes to the protocol
        const CARGO_PROTOCOL: (Name, Value) = (
            Name::from_static("cargo-protocol"),
            Value::from_static("version=1"),
        );
        // All index entries are just files with lines of JSON
        const ACCEPT: (Name, Value) = (header::ACCEPT, Value::from_static("text/plain"));
        // It's fine to send an empty etag that will always miss if we can't get the etag/last-modified from disk
        const UNKNOWN_CACHE: (Name, Value) = (header::IF_NONE_MATCH, Value::from_static(""));

        let cache = {
            // If we have a local cache entry, include its version with the
            // appropriate header, this allows the server to respond with a
            // cached, or even better, empty response if its version matches
            // the local one making the request/response loop basically free

            // If we're unable to get the cache version we can just ignore setting the
            // header, guaranteeing we'll get the full index contents if the crate exists
            let set_cache_version = || -> Option<(header::HeaderName, header::HeaderValue)> {
                let contents = self.cache.read_cache_file(name, lock).ok()??;
                let valid = ValidCacheEntry::read(&contents).ok()?;

                let (name, value) =
                    if let crate::index::cache::Revision::Http(http) = valid.revision {
                        match http {
                            HttpRevision::Etag(value) => (header::IF_NONE_MATCH, value.trim()),
                            HttpRevision::LastModified(value) => {
                                (header::IF_MODIFIED_SINCE, value.trim())
                            }
                        }
                    } else {
                        // We could error here, but that's kind of pointless
                        // since the response will be sent in full if we haven't
                        // specified one of the above headers. Though it does
                        // potentially indicate something weird is going on
                        return None;
                    };

                Some((name, header::HeaderValue::from_str(value.trim()).ok()?))
            };

            if let Some(rev) = revision {
                let (name, value) = match rev {
                    HttpRevision::Etag(etag) => (header::IF_NONE_MATCH, etag),
                    HttpRevision::LastModified(lm) => (header::IF_MODIFIED_SINCE, lm),
                };

                let hv =
                    header::HeaderValue::from_str(value.trim()).map_err(crate::HttpError::from)?;

                (name, hv)
            } else {
                // Use the etag (or last modified, though crates.io does not use this AFAICT)
                // from the cache entry if it exists
                set_cache_version().unwrap_or(UNKNOWN_CACHE)
            }
        };

        Ok(Request {
            uri,
            headers: [CARGO_PROTOCOL, ACCEPT, cache],
            method: http::Method::GET,
        })
    }

    /// Process the response to a request created by [`Self::make_remote_request`]
    ///
    /// This handles both the scenario where the local cache is missing the specified
    /// crate, or it is out of date, as well as the local entry being up to date
    /// and can just be read from disk
    ///
    /// You may specify whether an updated index entry is written locally to the
    /// cache or not
    ///
    /// Note that responses from sparse HTTP indices, at least crates.io, may
    /// send responses with `gzip` compression, it is your responsibility to
    /// decompress it before sending to this function
    pub fn parse_remote_response(
        &self,
        name: KrateName<'_>,
        response: Response<'_>,
        write_cache_entry: bool,
        lock: &FileLock,
    ) -> Result<Option<IndexKrate>, Error> {
        use http::StatusCode;

        match response.status_code {
            // The server responded with the full contents of the index entry
            StatusCode::OK => {
                let krate = IndexKrate::from_slice(response.body)?;

                if write_cache_entry {
                    // It's unfortunate if we can't write to the cache, but we
                    // don't treat it as a hard error since we still have the
                    // index metadata
                    let _err = self.cache.write_to_cache(
                        &krate,
                        &response.revision.map_or(
                            crate::index::cache::Revision::Unknown,
                            crate::index::cache::Revision::Http,
                        ),
                        lock,
                    );
                }

                Ok(Some(krate))
            }
            // The local cache entry is up to date with the latest entry on the
            // server, we can just return the local one
            StatusCode::NOT_MODIFIED => self.cache.cached_krate(name, None, lock),
            // The server requires authorization but the user didn't provide it
            StatusCode::UNAUTHORIZED => Err(HttpError::StatusCode {
                code: StatusCode::UNAUTHORIZED,
                msg: "the request was not authorized",
            }
            .into()),
            // The crate does not exist, or has been removed
            StatusCode::NOT_FOUND
            | StatusCode::GONE
            | StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS => Ok(None),
            code => Err(HttpError::StatusCode {
                code,
                msg: "the status code is invalid for this protocol",
            }
            .into()),
        }
    }
}

/// A utility that can be used to wrap remote requests/responses that caches the etag to avoid disk I/O
pub struct EtagCache {
    cache: Vec<(smol_str::SmolStr, String)>,
}

impl Default for EtagCache {
    fn default() -> Self {
        Self { cache: Vec::new() }
    }
}

impl EtagCache {
    #[inline]
    pub fn make_remote_request(
        &self,
        index: &SparseIndex,
        name: KrateName<'_>,
        lock: &FileLock,
    ) -> Result<Request, Error> {
        let etag = self
            .cache
            .binary_search_by_key(&name.0, |(kn, _)| kn.as_str())
            .ok()
            .map(|i| self.cache[i].1.as_str());

        index.make_remote_request(name, etag.map(HttpRevision::Etag), lock)
    }

    #[inline]
    pub fn parse_remote_response(
        &mut self,
        index: &SparseIndex,
        name: KrateName<'_>,
        response: Response<'_>,
        write_cache_entry: bool,
        lock: &FileLock,
    ) -> Result<Option<IndexKrate>, Error> {
        if let Some(HttpRevision::Etag(etag)) = &response.revision {
            let etag = (*etag).to_owned();
            match self
                .cache
                .binary_search_by_key(&name.0, |(krate, _)| krate.as_str())
            {
                Ok(i) => self.cache[i].1 = etag,
                Err(i) => self.cache.insert(i, (name.0.into(), etag)),
            }
        }

        index.parse_remote_response(name, response, write_cache_entry, lock)
    }
}
