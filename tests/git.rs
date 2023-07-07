#![cfg(feature = "git")]

mod utils;
use tame_index::{index::RemoteGitIndex, GitIndex, IndexKrate, IndexLocation, IndexPath, IndexUrl};

fn remote_index(
    path: impl AsRef<tame_index::Path>,
    url: impl AsRef<tame_index::Path>,
) -> RemoteGitIndex {
    RemoteGitIndex::new(
        GitIndex::new(IndexLocation {
            url: IndexUrl::NonCratesIo(url.as_ref().as_str()),
            root: IndexPath::Exact(path.as_ref().to_owned()),
        })
        .unwrap(),
    )
    .unwrap()
}

/// For testing purposes we create a local git repository as the remote for tests
/// so that we avoid
///
/// 1. Using the crates.io git registry. It's massive and slow.
/// 2. Using some other external git registry, could fail for any number of
/// network etc related issues
/// 3. Needing to maintain a blessed remote of any kind
struct FakeRemote {
    repo: gix::Repository,
    td: utils::TempDir,
    parent: gix::ObjectId,
    commits: u32,
}

impl FakeRemote {
    fn new() -> Self {
        let td = utils::tempdir();

        let mut repo = gix::init_bare(&td).expect("failed to create remote repo");

        // Create an empty initial commit so we always have _something_
        let parent = {
            let empty_tree_id = repo
                .write_object(&gix::objs::Tree::empty())
                .unwrap()
                .detach();

            let repo = Self::snapshot(&mut repo);
            repo.commit(
                "HEAD",
                "initial commit",
                empty_tree_id,
                gix::commit::NO_PARENT_IDS,
            )
            .unwrap()
            .detach()
        };

        Self {
            td,
            repo,
            parent,
            commits: 0,
        }
    }

    #[inline]
    fn snapshot(repo: &mut gix::Repository) -> gix::config::CommitAutoRollback<'_> {
        let mut config = repo.config_snapshot_mut();
        config
            .set_raw_value("author", None, "name", "Integration Test")
            .unwrap();
        config
            .set_raw_value("committer", None, "name", "Integration Test")
            .unwrap();
        config
            .set_raw_value("author", None, "email", "tests@integration.se")
            .unwrap();
        config
            .set_raw_value("committer", None, "email", "tests@integration.se")
            .unwrap();

        config.commit_auto_rollback().unwrap()
    }

    fn commit(&mut self, krate: &IndexKrate) -> gix::ObjectId {
        use gix::objs::{
            tree::{Entry, EntryMode},
            Tree,
        };
        let repo = Self::snapshot(&mut self.repo);

        let mut serialized = Vec::new();
        krate.write_json_lines(&mut serialized).unwrap();

        let name: tame_index::KrateName<'_> = krate.name().try_into().unwrap();
        let rel_path = tame_index::PathBuf::from(name.relative_path(None));

        let blob_id = repo.write_blob(serialized).unwrap().into();
        let mut tree = Tree::empty();
        tree.entries.push(Entry {
            mode: EntryMode::Blob,
            oid: blob_id,
            filename: rel_path.file_name().unwrap().into(),
        });

        let mut tree_id = repo.write_object(tree).unwrap().detach();

        let mut parent = rel_path.parent();
        // Now create all the parent trees to the root
        while let Some(filename) = parent.and_then(|p| p.file_name()) {
            let mut tree = Tree::empty();
            tree.entries.push(Entry {
                mode: EntryMode::Tree,
                oid: tree_id,
                filename: filename.into(),
            });

            tree_id = repo.write_object(tree).unwrap().detach();
            parent = parent.unwrap().parent();
        }

        self.commits += 1;

        let parent = repo
            .commit(
                "HEAD",
                format!("{} - {}", krate.name(), self.commits),
                tree_id,
                [self.parent],
            )
            .unwrap()
            .detach();
        self.parent = parent;
        parent
    }

    fn local(&self) -> (RemoteGitIndex, utils::TempDir) {
        let td = utils::tempdir();

        let rgi = remote_index(&td, &self.td);

        (rgi, td)
    }
}

/// Validates we can clone a new index repo
#[test]
fn clones_new() {
    let remote = FakeRemote::new();

    let (rgi, _td) = remote.local();

    assert!(rgi
        .cached_krate("clones_new".try_into().unwrap())
        .unwrap()
        .is_none());
}

/// Validates we can open an existing index repo
#[test]
fn opens_existing() {
    let mut remote = FakeRemote::new();

    let krate = utils::fake_krate("opens-existing", 4);
    let expected_head = remote.commit(&krate);

    let (first, td) = remote.local();

    assert_eq!(
        first.local().head_commit().unwrap(),
        expected_head.to_hex().to_string()
    );

    // This should not be in the cache
    assert_eq!(
        first
            .krate("opens-existing".try_into().unwrap(), true)
            .expect("failed to read git blob")
            .expect("expected krate"),
        krate,
    );

    let second = remote_index(&td, &remote.td);

    assert_eq!(
        second.local().head_commit().unwrap(),
        expected_head.to_hex().to_string()
    );

    // This should be in the cache as it is file based not memory based
    assert_eq!(
        first
            .local()
            .cached_krate("opens-existing".try_into().unwrap())
            .expect("failed to read cache file")
            .expect("expected cached krate"),
        krate,
    );
}

/// Validates that cache entries can be created and used
#[test]
fn updates_cache() {
    let mut remote = FakeRemote::new();

    let krate = utils::fake_krate("updates-cache", 4);
    let expected_head = remote.commit(&krate);

    let (rgi, _td) = remote.local();

    assert_eq!(
        rgi.local().head_commit().unwrap(),
        expected_head.to_hex().to_string()
    );

    // This should not be in the cache
    assert_eq!(
        rgi.krate("updates-cache".try_into().unwrap(), true)
            .expect("failed to read git blob")
            .expect("expected krate"),
        krate,
    );

    assert_eq!(
        rgi.local()
            .cached_krate("updates-cache".try_into().unwrap())
            .expect("failed to read cache file")
            .expect("expected krate"),
        krate,
    );
}

/// Validates we can fetch updates from the remote and invalidate cache entries
#[test]
fn fetch_invalidates_cache() {
    let mut remote = FakeRemote::new();

    let krate = utils::fake_krate("invalidates-cache", 4);
    let expected_head = remote.commit(&krate);

    let (mut rgi, _td) = remote.local();

    assert_eq!(
        rgi.local().head_commit().unwrap(),
        expected_head.to_hex().to_string()
    );

    // This should not be in the cache
    assert_eq!(
        rgi.krate("invalidates-cache".try_into().unwrap(), true)
            .expect("failed to read git blob")
            .expect("expected krate"),
        krate,
    );

    // Update the remote
    let new_krate = utils::fake_krate("invalidates-cache", 5);
    let new_head = remote.commit(&new_krate);

    assert_eq!(
        rgi.local()
            .cached_krate("invalidates-cache".try_into().unwrap())
            .expect("failed to read cache file")
            .expect("expected krate"),
        krate,
    );

    // Perform fetch, which should invalidate the cache
    rgi.fetch().unwrap();

    assert_eq!(
        rgi.local().head_commit().unwrap(),
        new_head.to_hex().to_string()
    );

    assert!(rgi
        .local()
        .cached_krate("invalidates-cache".try_into().unwrap())
        .unwrap()
        .is_none());

    assert_eq!(
        rgi.krate("invalidates-cache".try_into().unwrap(), true)
            .expect("failed to read git blob")
            .expect("expected krate"),
        new_krate,
    );

    // We haven't made new commits, so the fetch should not move HEAD and thus
    // cache entries should still be valid
    rgi.fetch().unwrap();

    assert_eq!(
        rgi.local()
            .cached_krate("invalidates-cache".try_into().unwrap())
            .unwrap()
            .unwrap(),
        new_krate
    );

    let krate3 = utils::fake_krate("krate-3", 3);
    remote.commit(&krate3);

    let krate4 = utils::fake_krate("krate-4", 4);
    let expected_head = remote.commit(&krate4);

    rgi.fetch().unwrap();

    assert_eq!(
        rgi.local().head_commit().unwrap(),
        expected_head.to_hex().to_string()
    );

    assert!(rgi
        .local()
        .cached_krate("invalidates-cache".try_into().unwrap())
        .unwrap()
        .is_none());
}

/// gix uses a default branch name of `main`, but most cargo git indexes on users
/// disks use the master branch, so just ensure that we support that as well
#[test]
fn non_main_local_branch() {
    let mut remote = FakeRemote::new();

    let local_td = utils::tempdir();

    // Set up the local repo as if it was an already existing index
    // created by cargo
    {
        // Do that actual init
        let mut cmd = std::process::Command::new("git");
        cmd.args(["init", "--bare", "-b", "master"]);
        cmd.arg(local_td.path());
        assert!(
            cmd.status().expect("failed to run git").success(),
            "git failed to init directory"
        );

        // Add the remote, we expect the remote to already be set if the repo exists
        let mut cmd = std::process::Command::new("git");
        cmd.arg("-C");
        cmd.arg(local_td.path());
        cmd.args(["remote", "add", "origin"]);
        cmd.arg(remote.td.path());
        assert!(
            cmd.status().expect("failed to run git").success(),
            "git failed to add remote"
        );

        // Add a fake commit so that we have a local HEAD
        let mut repo = gix::open(local_td.path()).unwrap();

        let commit = {
            let snap = FakeRemote::snapshot(&mut repo);
            let empty_tree_id = snap
                .write_object(&gix::objs::Tree::empty())
                .unwrap()
                .detach();

            snap.commit(
                "refs/heads/master",
                "initial commit",
                empty_tree_id,
                gix::commit::NO_PARENT_IDS,
            )
            .unwrap()
            .detach()
        };

        use gix::refs::transaction as tx;
        repo.edit_reference(tx::RefEdit {
            change: tx::Change::Update {
                log: tx::LogChange {
                    mode: tx::RefLog::AndReference,
                    force_create_reflog: false,
                    message: "".into(),
                },
                expected: tx::PreviousValue::Any,
                new: gix::refs::Target::Peeled(commit),
            },
            name: "refs/heads/master".try_into().unwrap(),
            deref: false,
        })
        .unwrap();

        repo.edit_reference(tx::RefEdit {
            change: tx::Change::Update {
                log: tx::LogChange {
                    mode: tx::RefLog::AndReference,
                    force_create_reflog: false,
                    message: "".into(),
                },
                expected: tx::PreviousValue::Any,
                new: gix::refs::Target::Symbolic("refs/heads/master".try_into().unwrap()),
            },
            name: "HEAD".try_into().unwrap(),
            deref: false,
        })
        .unwrap();

        assert_eq!(commit, repo.head_commit().unwrap().id);
    }

    let mut rgi = remote_index(&local_td, &remote.td);

    let first = utils::fake_krate("first", 1);
    remote.commit(&first);

    rgi.fetch().unwrap();

    assert_eq!(
        rgi.krate("first".try_into().unwrap(), true)
            .unwrap()
            .unwrap(),
        first
    );
}
