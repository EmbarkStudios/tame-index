use super::SparseIndex;
use crate::{Error, IndexKrate, KrateName};
pub use reqwest::blocking::Client;
pub use reqwest::Client as AsyncClient;

pub struct RemoteSparseIndex {
    index: SparseIndex,
    client: Client,
}

impl RemoteSparseIndex {
    /// Creates a new [`Self`] that can access and write local cache entries,
    /// and contact the remote index to retrieve the latest index information
    #[inline]
    pub fn new(index: SparseIndex, client: Client) -> Self {
        Self { index, client }
    }

    /// Splits into the component parts
    #[inline]
    pub fn into_parts(self) -> (SparseIndex, Client) {
        (self.index, self.client)
    }

    /// Gets the latest index metadata for the crate
    ///
    /// Network I/O is _always_ performed when calling this method, however the
    /// reponse from the remote registry will be empty of contents other than headers
    /// if the local cache entry for the crate is up to date with th latest in the index
    pub fn krate(
        &self,
        name: KrateName<'_>,
        write_cache_entry: bool,
    ) -> Result<Option<IndexKrate>, Error> {
        let req = self.index.make_remote_request(name)?;
        let req = req.try_into()?;

        let res = self.client.execute(req)?;

        let mut builder = http::Response::builder()
            .status(res.status())
            .version(res.version());

        builder
            .headers_mut()
            .unwrap()
            .extend(res.headers().iter().map(|(k, v)| (k.clone(), v.clone())));

        let body = res.bytes()?;
        let res = builder.body(body.to_vec())?;

        self.index
            .parse_remote_response(name, res, write_cache_entry)
    }

    /// Attempts to read the locally cached crate information
    ///
    /// This method does no network I/O unlike [`Self::krate`], but does not
    /// guarantee that the cache information is up to date with the latest in
    /// the remote index
    #[inline]
    pub fn cached_krate(&self, name: KrateName<'_>) -> Result<Option<IndexKrate>, Error> {
        self.index.cached_krate(name, None)
    }
}

pub struct AsyncRemoteSparseIndex {
    index: SparseIndex,
    client: AsyncClient,
}

impl AsyncRemoteSparseIndex {
    /// Creates a new [`Self`] that can access and write local cache entries,
    /// and contact the remote index to retrieve the latest index information
    #[inline]
    pub fn new(index: SparseIndex, client: AsyncClient) -> Self {
        Self { index, client }
    }

    /// Splits into the component parts
    #[inline]
    pub fn into_parts(self) -> (SparseIndex, AsyncClient) {
        (self.index, self.client)
    }

    /// Async version of [`RemoteSparseIndex::krate`]
    pub async fn krate_async(
        &self,
        name: KrateName<'_>,
        write_cache_entry: bool,
    ) -> Result<Option<IndexKrate>, Error> {
        let req = self.index.make_remote_request(name)?;
        let req = req.try_into()?;

        let res = self.client.execute(req).await?;

        let mut builder = http::Response::builder()
            .status(res.status())
            .version(res.version());

        builder
            .headers_mut()
            .unwrap()
            .extend(res.headers().iter().map(|(k, v)| (k.clone(), v.clone())));

        let body = res.bytes().await?;
        let res = builder.body(body.to_vec())?;

        self.index
            .parse_remote_response(name, res, write_cache_entry)
    }

    /// Attempts to read the locally cached crate information
    ///
    /// This method does no network I/O unlike [`Self::krate_async`], but does not
    /// guarantee that the cache information is up to date with the latest in
    /// the remote index
    #[inline]
    pub fn cached_krate(&self, name: KrateName<'_>) -> Result<Option<IndexKrate>, Error> {
        self.index.cached_krate(name, None)
    }
}

impl From<reqwest::Error> for Error {
    #[inline]
    fn from(e: reqwest::Error) -> Self {
        Self::Http(crate::HttpError::Reqwest(e))
    }
}

impl From<http::Error> for Error {
    #[inline]
    fn from(e: http::Error) -> Self {
        Self::Http(crate::HttpError::Http(e))
    }
}
