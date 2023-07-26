use super::GitIndex;
use crate::{Error, IndexKrate, KrateName};
use std::sync::atomic::AtomicBool;

/// Uses a "bare" git index that fetches files directly from the repo instead of
/// using a local checkout, the same as cargo itself.
///
/// Uses cargo's cache
pub struct RemoteGitIndex {
    index: GitIndex,
    repo: gix::Repository,
    remote_name: Option<String>,
}

impl RemoteGitIndex {
    /// Creates a new [`Self`] that can access and write local cache entries,
    /// and contact the remote index to retrieve the latest index information
    ///
    /// Note that if a repository does not exist at the local disk path of the
    /// provided [`GitIndex`], a full clone will be performed.
    #[inline]
    pub fn new(index: GitIndex) -> Result<Self, Error> {
        Self::with_options(index, gix::progress::Discard, &AtomicBool::default())
    }

    /// Breaks [`Self`] into its component parts
    ///
    /// This method is useful if you need thread safe access to the repository
    #[inline]
    pub fn into_parts(self) -> (GitIndex, gix::Repository) {
        (self.index, self.repo)
    }

    /// Creates a new [`Self`] that allows showing of progress of the the potential
    /// fetch if the disk location is empty, as well as allowing interruption
    /// of the fetch operation
    pub fn with_options<P>(
        mut index: GitIndex,
        progress: P,
        should_interrupt: &AtomicBool,
    ) -> Result<Self, Error>
    where
        P: gix::Progress,
        P::SubProgress: 'static,
    {
        let open_or_clone_repo = || -> Result<_, GitError> {
            match gix::open(&index.cache.path) {
                Ok(repo) => Ok(repo),
                Err(gix::open::Error::NotARepository { .. }) => {
                    let (repo, _out) =
                        gix::prepare_clone_bare(index.url.as_str(), &index.cache.path)
                            .map_err(Box::new)?
                            .fetch_only(progress, should_interrupt)?;
                    Ok(repo)
                }
                Err(err) => Err(Box::new(err).into()),
            }
        };

        let mut repo = open_or_clone_repo()?;
        repo.object_cache_size_if_unset(4 * 1024 * 1024);
        let remote_name = repo.remote_names().into_iter().next().map(String::from);

        Self::set_head(&mut index, &repo)?;

        Ok(Self {
            repo,
            index,
            remote_name,
        })
    }

    /// Gets the local index
    #[inline]
    pub fn local(&self) -> &GitIndex {
        &self.index
    }

    /// Get the configuration of the index.
    ///
    /// See the [cargo docs](https://doc.rust-lang.org/cargo/reference/registry-index.html#index-configuration)
    pub fn index_config(&self) -> Result<super::IndexConfig, Error> {
        let blob = self.read_blob("config.json")?.ok_or_else(|| {
            Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "unable to find config.json",
            ))
        })?;
        Ok(serde_json::from_slice(&blob.data)?)
    }

    /// Sets the head commit in the wrapped index so that cache entries can be
    /// properly filtered
    #[inline]
    fn set_head(index: &mut GitIndex, repo: &gix::Repository) -> Result<(), Error> {
        let head = repo.head_commit().ok().map(|head| {
            let gix::ObjectId::Sha1(sha1) = head.id;
            sha1
        });

        index.set_head_commit(head);

        Ok(())
    }

    /// Attempts to read the specified crate's index metadata
    ///
    /// An attempt is first made to read the cache entry for the crate, and
    /// falls back to reading the metadata from the git blob it is stored in
    ///
    /// This method does no network I/O
    pub fn krate(
        &self,
        name: KrateName<'_>,
        write_cache_entry: bool,
    ) -> Result<Option<IndexKrate>, Error> {
        if let Ok(Some(cached)) = self.cached_krate(name) {
            return Ok(Some(cached));
        }

        let Some(blob) = self.read_blob(&name.relative_path(None))? else { return Ok(None) };

        let krate = IndexKrate::from_slice(&blob.data)?;
        if write_cache_entry {
            // It's unfortunate if fail to write to the cache, but we still were
            // able to retrieve the contents from git
            let mut hex_id = [0u8; 40];
            let gix::ObjectId::Sha1(sha1) = blob.id;
            let blob_id = crate::utils::encode_hex(&sha1, &mut hex_id);

            let _ = self.index.write_to_cache(&krate, Some(blob_id));
        }

        Ok(Some(krate))
    }

    fn read_blob(&self, path: &str) -> Result<Option<gix::ObjectDetached>, GitError> {
        let tree = self.repo.head_commit()?.tree()?;

        let mut buf = Vec::new();
        let Some(entry) = tree.lookup_entry_by_path(path, &mut buf).map_err(|err| GitError::BlobLookup(Box::new(err)))? else { return Ok(None) };
        let blob = entry
            .object()
            .map_err(|err| GitError::BlobLookup(Box::new(err)))?;

        // Sanity check this is a blob, it _shouldn't_ be possible to get anything
        // else (like a subtree), but better safe than sorry
        if blob.kind != gix::object::Kind::Blob {
            return Ok(None);
        }

        Ok(Some(blob.detach()))
    }

    /// Attempts to read the locally cached crate information
    ///
    /// Note this method has improvements over using [`GitIndex::cached_krate`].
    ///
    /// In older versions of cargo, only the head commit hash is used as the version
    /// for cached crates, which means a fetch invalidates _all_ cached crates,
    /// even if they have not been modified in any commits since the previous
    /// fetch.
    ///
    /// This method does the same thing as cargo, which is to allow _either_
    /// the head commit oid _or_ the blob oid as a version, which is more
    /// granular and means the cached crate can remain valid as long as it is
    /// not updated in a subsequent fetch. [`GitIndex::cached_krate`] cannot take
    /// advantage of that though as it does not have access to git and thus
    /// cannot know the blob id.
    #[inline]
    pub fn cached_krate(&self, name: KrateName<'_>) -> Result<Option<IndexKrate>, Error> {
        let Some(cached) = self.index.cache.read_cache_file(name)? else { return Ok(None) };
        let valid = crate::index::cache::ValidCacheEntry::read(&cached)?;

        if Some(valid.revision) != self.index.head_commit() {
            let Some(blob) = self.read_blob(&name.relative_path(None))? else { return Ok(None) };

            let mut hex_id = [0u8; 40];
            let gix::ObjectId::Sha1(sha1) = blob.id;
            let blob_id = crate::utils::encode_hex(&sha1, &mut hex_id);

            if valid.revision != blob_id {
                return Ok(None);
            }
        }

        valid.to_krate(None)
    }

    /// Performs a fetch from the remote index repository.
    ///
    /// This method performs network I/O.
    #[inline]
    pub fn fetch(&mut self) -> Result<(), Error> {
        self.fetch_with_options(gix::progress::Discard, &AtomicBool::default())
    }

    /// Same as [`Self::fetch`] but allows specifying a progress implementation
    /// and allows interruption of the network operations
    pub fn fetch_with_options<P>(
        &mut self,
        mut progress: P,
        should_interrupt: &AtomicBool,
    ) -> Result<(), Error>
    where
        P: gix::Progress,
        P::SubProgress: 'static,
    {
        const DIR: gix::remote::Direction = gix::remote::Direction::Fetch;

        let mut perform_fetch = || -> Result<(), GitError> {
            // Attempt to lookup the remote we _think_ we should use first,
            // otherwise fallback to getting the remote for the current HEAD
            let mut remote = if let Some(remote) = self
                .remote_name
                .as_deref()
                .and_then(|name| self.repo.find_remote(name).ok())
            {
                remote
            } else {
                self.repo
                    .head()
                    .map_err(Box::new)?
                    .into_remote(DIR)
                    .map(|r| r.map_err(|e| GitError::RemoteLookup(Box::new(e))))
                    .or_else(|| {
                        self.repo
                            .find_default_remote(DIR)
                            .map(|r| r.map_err(|e| GitError::RemoteLookup(Box::new(e))))
                    })
                    .unwrap_or_else(|| Err(GitError::UnknownRemote))?
            };

            let remote_head = format!(
                "refs/remotes/{}/HEAD",
                self.remote_name.as_deref().unwrap_or("origin")
            );

            remote
                .replace_refspecs(Some(format!("HEAD:{remote_head}").as_str()), DIR)
                .expect("valid statically known refspec");

            // Perform the actual fetch
            let fetch_response: gix::remote::fetch::Outcome = remote
                .connect(DIR)
                .map_err(Box::new)?
                .prepare_fetch(&mut progress, Default::default())
                .map_err(Box::new)?
                .receive(&mut progress, should_interrupt)?;

            // Find the commit id of the remote's HEAD
            let remote_head_id = fetch_response.ref_map.mappings.iter().find_map(|mapping| {
                let gix::remote::fetch::Source::Ref(rref) = &mapping.remote else { return None; };

                if mapping.local.as_deref()? != remote_head.as_bytes() {
                    return None;
                }

                let gix::protocol::handshake::Ref::Symbolic {
                    full_ref_name,
                    object,
                    ..
                } = rref else { return None; };

                (full_ref_name == "HEAD").then_some(*object)
            }).ok_or(GitError::UnableToFindRemoteHead)?;

            use gix::refs::{transaction as tx, Target};

            // In all (hopefully?) cases HEAD is a symbolic reference to
            // refs/heads/<branch> which is a peeled commit id, if that's the case
            // we update it to the new commit id, otherwise we just set HEAD
            // directly
            use gix::head::Kind;
            let edit = match self.repo.head().map_err(Box::new)?.kind {
                Kind::Symbolic(sref) => {
                    // Update our local HEAD to the remote HEAD
                    if let Target::Symbolic(name) = sref.target {
                        Some(tx::RefEdit {
                            change: tx::Change::Update {
                                log: tx::LogChange {
                                    mode: tx::RefLog::AndReference,
                                    force_create_reflog: false,
                                    message: "".into(),
                                },
                                expected: tx::PreviousValue::MustExist,
                                new: gix::refs::Target::Peeled(remote_head_id),
                            },
                            name,
                            deref: true,
                        })
                    } else {
                        None
                    }
                }
                Kind::Unborn(_) | Kind::Detached { .. } => None,
            };

            self.repo
                .edit_reference(edit.unwrap_or_else(|| tx::RefEdit {
                    change: tx::Change::Update {
                        log: tx::LogChange {
                            mode: tx::RefLog::AndReference,
                            force_create_reflog: false,
                            message: "".into(),
                        },
                        expected: tx::PreviousValue::Any,
                        new: gix::refs::Target::Peeled(remote_head_id),
                    },
                    name: "HEAD".try_into().unwrap(),
                    deref: true,
                }))?;

            // Sanity check that the local HEAD points to the same commit
            // as the remote HEAD
            if remote_head_id != self.repo.head_commit()?.id {
                Err(GitError::UnableToUpdateHead)
            } else {
                Ok(())
            }
        };

        perform_fetch()?;
        Self::set_head(&mut self.index, &self.repo)?;

        Ok(())
    }
}

/// Errors that can occur during a git operation
#[derive(Debug, thiserror::Error)]
#[allow(missing_docs)]
pub enum GitError {
    #[error(transparent)]
    ClonePrep(#[from] Box<gix::clone::Error>),
    #[error(transparent)]
    CloneFetch(#[from] gix::clone::fetch::Error),
    #[error(transparent)]
    Connect(#[from] Box<gix::remote::connect::Error>),
    #[error(transparent)]
    FetchPrep(#[from] Box<gix::remote::fetch::prepare::Error>),
    #[error(transparent)]
    Fetch(#[from] gix::remote::fetch::Error),
    #[error(transparent)]
    Open(#[from] Box<gix::open::Error>),
    #[error(transparent)]
    Peel(#[from] gix::reference::peel::Error),
    #[error(transparent)]
    Head(#[from] gix::reference::head_commit::Error),
    #[error(transparent)]
    HeadUpdate(#[from] gix::reference::edit::Error),
    #[error(transparent)]
    Commit(#[from] gix::object::commit::Error),
    #[error(transparent)]
    ReferenceLookup(#[from] Box<gix::reference::find::existing::Error>),
    #[error(transparent)]
    BlobLookup(#[from] Box<gix::odb::find::existing::Error<gix::odb::store::find::Error>>),
    #[error(transparent)]
    RemoteLookup(#[from] Box<gix::remote::find::existing::Error>),
    #[error("unable to determine a suitable remote")]
    UnknownRemote,
    #[error("unable to locate remote HEAD")]
    UnableToFindRemoteHead,
    #[error("unable to update HEAD to remote HEAD")]
    UnableToUpdateHead,
}

impl GitError {
    /// Returns true if the error is a (potentially) spurious network error that
    /// indicates a retry of the operation could succeed
    #[inline]
    pub fn is_spurious(&self) -> bool {
        use gix::protocol::transport::IsSpuriousError;

        if let Self::Fetch(fe) | Self::CloneFetch(gix::clone::fetch::Error::Fetch(fe)) = self {
            fe.is_spurious()
        } else {
            false
        }
    }

    /// Returns true if a fetch could not be completed successfully due to the
    /// repo being locked, and could succeed if retried
    #[inline]
    pub fn is_locked(&self) -> bool {
        if let Self::Fetch(gix::remote::fetch::Error::UpdateRefs(ure))
        | Self::CloneFetch(gix::clone::fetch::Error::Fetch(
            gix::remote::fetch::Error::UpdateRefs(ure),
        )) = self
        {
            if let gix::remote::fetch::refs::update::Error::EditReferences(ere) = ure {
                return match ere {
                    gix::reference::edit::Error::FileTransactionPrepare(ftpe) => {
                        use gix::refs::file::transaction::prepare::Error as PrepError;
                        if let PrepError::LockAcquire { source, .. }
                        | PrepError::PackedTransactionAcquire(source) = ftpe
                        {
                            // currently this is either io or permanentlylocked, but just in case
                            // more variants are added, we just assume it's possible to retry
                            // in anything but the permanentlylocked variant
                            !matches!(source, gix::lock::acquire::Error::PermanentlyLocked { .. })
                        } else {
                            false
                        }
                    }
                    gix::reference::edit::Error::FileTransactionCommit(ftce) => {
                        matches!(
                            ftce,
                            gix::refs::file::transaction::commit::Error::LockCommit { .. }
                        )
                    }
                    _ => false,
                };
            }
        }

        false
    }
}
