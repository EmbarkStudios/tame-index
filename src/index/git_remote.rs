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
    branch_name: &'static str,
}

impl RemoteGitIndex {
    /// Creates a new [`Self`] that can access and write local cache entries,
    /// and contact the remote index to retrieve the latest index information
    #[inline]
    pub fn new(index: GitIndex) -> Result<Self, Error> {
        Self::with_options(index, gix::progress::Discard, &AtomicBool::default())
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
                        gix::prepare_clone_bare(index.url.as_str(), &index.cache.path)?
                            .fetch_only(progress, should_interrupt)?;
                    Ok(repo)
                }
                Err(err) => Err(err.into()),
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
            branch_name: "master",
        })
    }

    /// Sets the head commit in the wrapped index so that cache entries can be
    /// properly filtered
    #[inline]
    fn set_head(index: &mut GitIndex, repo: &gix::Repository) -> Result<(), Error> {
        // let head_ref = repo
        //     .find_reference("FETCH_HEAD")
        //     .or_else(|_e| repo.find_referenc("HEAD"))?;

        // let mut head_ref = match head_ref.inner.target {
        //     gix::gix_ref::TargetRef::Symbolic(branch) => match repo.find_reference(&branch) {
        //         Ok(r) => gix::head::Kind::Symbolic(r.detach()),
        //         Err(gix::reference::find::existing::Error::NotFound) => {
        //             gix::head::Kind::Unborn(branch)
        //         }
        //         Err(err) => return Err(err.into()),
        //     },
        //     gix::gix_ref::TargetRef::Peeled(target) => gix::head::Kind::Detached {
        //         target,
        //         peeled: head.inner.peeled,
        //     },
        // }
        // .attach(repo);

        // let head = head_ref.peel_to_commit_in_place()?;

        let head = repo.head_commit().map_err(GitError::Head)?;
        let gix::ObjectId::Sha1(sha1) = head.id;
        index.set_head_commit(Some(sha1));

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

        let get_blob = || -> Result<Option<Vec<u8>>, GitError> {
            let tree = self.repo.head_commit()?.tree()?;
            let relative_path = name.relative_path(None);

            let Some(entry) = tree.lookup_entry_by_path(&relative_path).map_err(GitError::BlobLookup)? else { return Ok(None) };
            let blob = entry.object().map_err(GitError::BlobLookup)?;

            // Sanity check this is a blob, it _shouldn't_ be possible to get anything
            // else (like a subtree), but better safe than sorry
            if blob.kind != gix::object::Kind::Blob {
                return Ok(None);
            }

            let blob = blob.detach();

            Ok(Some(blob.data))
        };

        let Some(blob) = get_blob()? else { return Ok(None) };

        let krate = IndexKrate::from_slice(&blob)?;
        if write_cache_entry {
            // It's unfortunate if fail to write to the cache, but we still were
            // able to retrieve the contents from git
            let _ = self.index.write_to_cache(&krate);
        }

        Ok(Some(krate))
    }

    /// Attempts to read the locally cached crate information
    #[inline]
    pub fn cached_krate(&self, name: KrateName<'_>) -> Result<Option<IndexKrate>, Error> {
        self.index.cached_krate(name)
    }

    /// Performs a fetch from the remote index repository.
    ///
    /// This method performs network I/O.
    ///
    /// If there is a new remote HEAD, this will invalidate all local cache entries
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
            // Attempt to lookup the remote we _think_ we should use first, otherwise
            // fallback to getting the remote for the current HEAD
            let mut remote = if let Some(remote) = self
                .remote_name
                .as_deref()
                .and_then(|name| self.repo.find_remote(name).ok())
            {
                remote
            } else {
                self.repo
                    .head()?
                    .into_remote(DIR)
                    .map(|r| r.map_err(GitError::RemoteLookup))
                    .or_else(|| {
                        self.repo
                            .find_default_remote(DIR)
                            .map(|r| r.map_err(GitError::RemoteLookup))
                    })
                    .unwrap_or_else(|| Err(GitError::UnknownRemote))?
            };

            if remote.refspecs(DIR).is_empty() {
                let spec = format!(
                    "+refs/heads/{branch}:refs/remotes/{remote}/{branch}",
                    remote = self.remote_name.as_deref().unwrap_or("origin"),
                    branch = self.branch_name,
                );
                remote
                    .replace_refspecs(Some(spec.as_str()), DIR)
                    .expect("valid statically known refspec");
            }
            let res: gix::remote::fetch::Outcome = remote
                .connect(DIR)?
                .prepare_fetch(&mut progress, Default::default())?
                .receive(&mut progress, should_interrupt)?;
            let branch_name = format!("refs/heads/{}", self.branch_name);
            let local_tracking = res
                .ref_map
                .mappings
                .iter()
                .find_map(|m| {
                    let gix::remote::fetch::Source::Ref(r) = &m.remote else { return None; };
                    (r.unpack().0 == branch_name)
                        .then_some(m.local.as_ref())
                        .flatten()
                })
                .ok_or(GitError::MissingLocalBranch {
                    branch: branch_name,
                })?;
            let id = self
                .repo
                .find_reference(local_tracking)
                .expect("local tracking branch exists if we see it here")
                .id()
                .detach();

            assert_eq!(id, self.repo.head_commit()?.id);
            Ok(())
        };

        perform_fetch()?;
        Self::set_head(&mut self.index, &self.repo)?;

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error(transparent)]
    ClonePrep(#[from] gix::clone::Error),
    #[error(transparent)]
    CloneFetch(#[from] gix::clone::fetch::Error),
    #[error(transparent)]
    Connect(#[from] gix::remote::connect::Error),
    #[error(transparent)]
    FetchPrep(#[from] gix::remote::fetch::prepare::Error),
    #[error(transparent)]
    Fetch(#[from] gix::remote::fetch::Error),
    #[error(transparent)]
    Open(#[from] gix::open::Error),
    #[error("unable to find local tracking branch '{branch}'")]
    MissingLocalBranch { branch: String },
    #[error(transparent)]
    Head(#[from] gix::reference::head_commit::Error),
    #[error(transparent)]
    Commit(#[from] gix::object::commit::Error),
    #[error(transparent)]
    ReferenceLookup(#[from] gix::reference::find::existing::Error),
    #[error(transparent)]
    BlobLookup(#[from] gix::odb::find::existing::Error<gix::odb::store::find::Error>),
    #[error(transparent)]
    RemoteLookup(#[from] gix::remote::find::existing::Error),
    #[error("unable to determine a suitable remote")]
    UnknownRemote,
}
