use crate::{Error, IndexKrate, KrateName, PathBuf};

/// The [`IndexCache`] allows access to the local cache entries for a remote index
///
/// This implementation does no network I/O whatsoever, but does do disk I/O
pub struct IndexCache {
    /// The root disk location of the local index
    pub(super) path: PathBuf,
}

impl IndexCache {
    /// Creates a local index exactly at the specified path
    #[inline]
    pub fn at_path(path: PathBuf) -> Self {
        Self { path }
    }

    /// Reads a crate from the local cache of the index.
    ///
    /// You may optionally pass in the revision the cache entry is expected to
    /// have, if it does match the cache entry will be ignored and an error returned
    #[inline]
    pub fn cached_krate(
        &self,
        name: KrateName<'_>,
        revision: Option<&str>,
    ) -> Result<Option<IndexKrate>, Error> {
        let Some(contents) = self.read_cache_file(name)? else { return Ok(None) };

        let valid = crate::cache::ValidCacheEntry::read(&contents)?;
        valid.to_krate(revision)
    }

    /// Writes the specified crate and revision to the cache
    pub fn write_to_cache(&self, krate: &IndexKrate, revision: &str) -> Result<PathBuf, Error> {
        let name = krate.name().try_into()?;
        let cache_path = self.cache_path(name);

        std::fs::create_dir_all(cache_path.parent().unwrap())?;

        let mut cache_file = match std::fs::File::create(&cache_path) {
            Ok(cf) => cf,
            Err(err) => return Err(Error::IoPath(err, cache_path)),
        };

        // It's unfortunate if this fails for some reason, but
        // not writing the cache entry shouldn't stop the user
        // from getting the crate's metadata
        match krate.write_cache_entry(&mut cache_file, revision) {
            Ok(_) => Ok(cache_path),
            Err(err) => {
                drop(cache_file);
                // _attempt_ to delete the file, to clean up after ourselves
                let _ = std::fs::remove_file(&cache_path);
                Err(Error::IoPath(err, cache_path))
            }
        }
    }

    /// Gets the path the crate's cache file would be located at if it exists
    #[inline]
    pub(super) fn cache_path(&self, name: KrateName<'_>) -> PathBuf {
        let rel_path = name.relative_path(None);

        // avoid realloc on each push
        let mut cache_path = PathBuf::with_capacity(self.path.as_str().len() + 8 + rel_path.len());
        cache_path.push(&self.path);
        cache_path.push(".cache");
        cache_path.push(rel_path);

        cache_path
    }

    /// Attempts to read the cache entry for the specified crate
    pub(super) fn read_cache_file(&self, name: KrateName<'_>) -> Result<Option<Vec<u8>>, Error> {
        let cache_path = self.cache_path(name);

        let cache_bytes = match std::fs::read(&cache_path) {
            Ok(cb) => cb,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(Error::IoPath(err, cache_path)),
        };

        Ok(Some(cache_bytes))
    }
}
