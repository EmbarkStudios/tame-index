use super::{cache::ValidCacheEntry, IndexCache};
use crate::{utils::cargo_home, Error, HttpError, IndexKrate, KrateName, PathBuf};

/// The default URL of the crates.io HTTP index, see [`SparseIndex::with_url`],
/// [`SparseIndex::with_path`], or [`SparseIndex::crates_io`]
pub const CRATES_IO_HTTP_INDEX: &str = "sparse+https://index.crates.io/";

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
    /// Creates a view over the sparse HTTP index from a provided URL
    ///
    /// Note this opens the same location on disk that Cargo uses for that
    /// registry index's metadata and cache.
    ///
    /// Use [`Self::with_path`] if you wish to override the disk location
    #[inline]
    pub fn with_url(url: &str) -> Result<Self, Error> {
        Self::with_path(cargo_home()?, url)
    }

    /// Creates an index for the default crates.io registry, using the same
    /// disk location as Cargo itself.
    ///
    /// This is the recommended way to access the crates.io sparse index.
    #[inline]
    pub fn crates_io() -> Result<Self, Error> {
        Self::with_url(CRATES_IO_HTTP_INDEX)
    }

    /// Creates a view over the sparse HTTP index from the provided URL, rooted
    /// at the specified location
    ///
    /// Use this method if you wish to create a sparse index with the canonical
    /// cargo directory layout, but rooted at a location other than the default
    #[inline]
    pub fn with_path(root: impl Into<PathBuf>, url: impl AsRef<str>) -> Result<Self, Error> {
        let url = url.as_ref();
        // It is required to have the sparse+ scheme modifier for sparse urls as
        // they are part of the short ident hash calculation done by cargo
        if !url.starts_with("sparse+http") {
            return Err(crate::InvalidUrl {
                url: url.to_owned(),
                source: crate::InvalidUrlError::MissingSparse,
            }
            .into());
        }

        let (path, url) = crate::utils::get_index_details(url, Some(root.into()))?;
        Ok(Self::at_path(path, url))
    }

    /// Creates a local index exactly at the specified path for the specified remote url
    #[inline]
    pub fn at_path(path: PathBuf, mut url: String) -> Self {
        if !url.ends_with('/') {
            url.push('/');
        }

        Self {
            cache: IndexCache::at_path(path),
            url,
        }
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
    pub fn cached_krate(&self, name: KrateName<'_>) -> Result<Option<IndexKrate>, Error> {
        self.cache.cached_krate(name, None)
    }

    /// Creates an HTTP request that can be sent via your HTTP client of choice
    /// to retrieve the current metadata for the specified crate
    ///
    /// See [`Self::parse_remote_response`] processing the response from the remote
    /// index
    ///
    /// It is highly recommended to assume HTTP/2 when making requests to remote
    /// indices, at least crates.io
    pub fn make_remote_request(
        &self,
        name: KrateName<'_>,
    ) -> Result<http::Request<&'static [u8]>, Error> {
        use http::header;

        let url = self.crate_url(name);

        let mut req = http::Request::get(url).version(http::Version::HTTP_2);

        {
            let headers = req.headers_mut().unwrap();

            // AFAICT this does not affect responses at the moment, but could in
            // the future if there are changes to the protocol
            headers.insert(
                "cargo-protocol",
                header::HeaderValue::from_static("version=1"),
            );
            // All index entries are just files with lines of JSON
            headers.insert(
                header::ACCEPT,
                header::HeaderValue::from_static("text/plain"),
            );
            // We need to accept both identity and gzip, as otherwise cloudfront will
            // always respond to requests with strong etag's, which will differ from
            // cache entries generated by cargo
            headers.insert(
                header::ACCEPT_ENCODING,
                header::HeaderValue::from_static("gzip,identity"),
            );

            // If we have a local cache entry, include its version with the
            // appropriate header, this allows the server to respond with a
            // cached, or even better, empty response if its version matches
            // the local one making the request/response loop basically free

            // If we're unable to get the cache version we can just ignore setting the
            // header, guaranteeing we'll get the full index contents if the crate exists
            let set_cache_version = |headers: &mut header::HeaderMap| -> Option<()> {
                let contents = self.cache.read_cache_file(name).ok()??;
                let valid = ValidCacheEntry::read(&contents).ok()?;

                let (key, value) = valid.revision.split_once(':')?;
                let value = header::HeaderValue::from_str(value.trim()).ok()?;
                let name = if key == header::ETAG {
                    header::IF_NONE_MATCH
                } else if key == header::LAST_MODIFIED {
                    header::IF_MODIFIED_SINCE
                } else {
                    // We could error here, but that's kind of pointless
                    // since the response will be sent in full if we haven't
                    // specified one of the above headers. Though it does
                    // potentially indicate something weird is going on
                    return None;
                };

                headers.insert(name, value);
                None
            };

            let _ = set_cache_version(headers);
        }

        const EMPTY: &[u8] = &[];
        Ok(req.body(EMPTY).unwrap())
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
        response: http::Response<Vec<u8>>,
        write_cache_entry: bool,
    ) -> Result<Option<IndexKrate>, Error> {
        use http::{header, StatusCode};
        let (parts, body) = response.into_parts();

        match parts.status {
            // The server responded with the full contents of the index entry
            StatusCode::OK => {
                let krate = IndexKrate::from_slice(&body)?;

                if write_cache_entry {
                    // The same as cargo, prefer etag over last-modified
                    let version = if let Some(etag) = parts.headers.get(header::ETAG) {
                        etag.to_str()
                            .ok()
                            .map(|etag| format!("{}: {etag}", header::ETAG))
                    } else if let Some(lm) = parts.headers.get(header::LAST_MODIFIED) {
                        lm.to_str()
                            .ok()
                            .map(|lm| format!("{}: {lm}", header::LAST_MODIFIED))
                    } else {
                        None
                    };

                    let revision = version.unwrap_or_else(|| "Unknown".to_owned());

                    // It's unfortunate if we can't write to the cache, but we
                    // don't treat it as a hard error since we still have the
                    // index metadata
                    let _err = self.cache.write_to_cache(&krate, &revision);
                }

                Ok(Some(krate))
            }
            // The local cache entry is up to date with the latest entry on the
            // server, we can just return the local one
            StatusCode::NOT_MODIFIED => self.cache.cached_krate(name, None),
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
