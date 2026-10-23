#![cfg(target_pointer_width = "64")]

//! Note we only run these tests if _built_ for 64-bit, as the only test targets
//! this project cares about are 64 bit, and the issue is that, to match cargo
//! we need to calculate the hash of the index the same, the rub is that when
//! running a 32-bit target (eg. i686) on a 64-bit host where the cargo on the
//! host is built for the 64-bit target as well, the local directories won't match

use tame_index::{
    IndexCache,
    index::cache::{CacheWrite, ValidCacheEntry},
    utils::get_index_details,
};

/// Validates that we can parse the current version of cache entries emitted by cargo by reading the cache entry that
/// should be there for one of this crate's dependencies.
///
/// This test should only fail if you have built the crate, wiped the crates.io cache, then run the test executable
/// without using cargo. If you do that, that is your fault.
#[test]
fn parses_current_cargo_cache() {
    let stable = tame_index::utils::cargo_version(None).unwrap() >= semver::Version::new(1, 85, 0);
    let (path, _url) = get_index_details(tame_index::CRATES_IO_HTTP_INDEX, None, stable).unwrap();
    let cache = IndexCache::at_path(path);
    let lock = &integ::unlocked();

    let cached = cache
        .read_cache_file("camino".try_into().unwrap(), lock)
        .expect("failed to read cache file")
        .expect("cache file not found");

    assert!(
        matches!(
            cached.cached().unwrap().unwrap().revision,
            tame_index::index::cache::Revision::Http(
                tame_index::index::sparse::HttpRevision::Etag(_)
            )
        ),
        "cargo rarely/never writes anything but etag revisions to the cache"
    );

    let chksum = cached
        .iter()
        .expect("failed to open entry")
        .find_map(|version| {
            // The initial version of camino used by this crate, this version will _always_
            // exist in the index, even if it yanked in the future, and is otherwise immutable
            (version.raw().0 == "1.1.4").then(|| {
                version
                    .deserialize()
                    .expect("failed to deserialize 1.1.4")
                    .checksum
                    .to_string()
            })
        })
        .expect("failed to find 1.1.4");

    assert_eq!(
        "c530edf18f37068ac2d977409ed5cd50d53d73bc653c7647b48eb78976ac9ae2",
        chksum,
    );
}

/// Validates we can write cache files exactly the same as the current version of cargo
#[test]
fn serializes_current_cargo_cache() {
    let stable = tame_index::utils::cargo_version(None).unwrap() >= semver::Version::new(1, 85, 0);
    let (path, _url) = get_index_details(tame_index::CRATES_IO_HTTP_INDEX, None, stable).unwrap();
    let cache = IndexCache::at_path(path);
    let lock = &integ::unlocked();

    let cargos = cache
        .read_cache_file("camino".try_into().unwrap(), lock)
        .expect("failed to read cache file")
        .expect("cache file not found");

    let cargo_ce = cargos.cached().unwrap().unwrap();

    let check = |ours: &[u8]| {
        let our_ce = ValidCacheEntry::read(ours).expect("failed to parse our cache file");

        assert_eq!(cargo_ce.revision, our_ce.revision);

        let mut cargos = tame_index::utils::split(cargo_ce.version_entries, 0);
        let mut ours = tame_index::utils::split(our_ce.version_entries, 0);

        while let Some(cv) = cargos.next() {
            let cv = std::str::from_utf8(cv).unwrap();
            let Some(ov) = ours.next() else {
                panic!("cargo had version '{cv}' but we did not");
            };

            let ov = std::str::from_utf8(ov).unwrap();
            assert_eq!(cv, ov, "version mismatch");

            match (cargos.next(), ours.next()) {
                (Some(c), Some(v)) => {
                    if c.len() != v.len() {
                        panic!(
                            "cargo '{cv}' had {} bytes but we had {} bytes",
                            c.len(),
                            v.len()
                        );
                    } else {
                        assert_eq!(c, v);
                    }
                }
                (Some(c), None) => {
                    panic!(
                        "cargo '{cv}' had an entry of {} bytes but we had none",
                        c.len()
                    );
                }
                (None, _) => {
                    panic!("cargo version '{cv}' didn't have an entry",)
                }
            }
        }

        let remaining = ours.count() / 2;
        assert_eq!(remaining, 0, "we had entries cargo did not");
    };

    let mut ours = Vec::new();

    // Pretend this is a remote response
    {
        let mut r = Vec::with_capacity(20 * 1024);
        for entry in cargos.iter().unwrap() {
            r.extend_from_slice(entry.raw().1);
            r.push(b'\n');
        }

        CacheWrite::from(tame_index::index::JsonLines::validate(&r).unwrap())
            .write_cache_entry(&mut ours, &cargo_ce.revision)
            .unwrap();
        check(&ours);
    }

    ours.clear();
    {
        let ik = cargos.deserialize().unwrap();
        CacheWrite::from(&ik)
            .write_cache_entry(&mut ours, &cargo_ce.revision)
            .unwrap();
        check(&ours);
    }
}

#[test]
#[ignore = "deserializes every cache entry on the local system"]
fn deserialize_all() {
    let (root, _) = tame_index::IndexLocation::new(tame_index::IndexUrl::CratesIoSparse)
        .into_parts()
        .unwrap();
    let cio = tame_index::SparseIndex::new(tame_index::IndexLocation::new(
        tame_index::IndexUrl::CratesIoSparse,
    ))
    .unwrap();

    struct Context {
        index: tame_index::SparseIndex,
        total: usize,
        failed: std::collections::BTreeMap<String, String>,
    }

    fn walk_dir(path: &tame_index::Path, ctx: &mut Context) {
        let l = tame_index::flock::FileLock::unlocked();
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = tame_index::PathBuf::from_path_buf(entry.path()).unwrap();
            let ft = entry.file_type().unwrap();

            if ft.is_dir() {
                walk_dir(&path, ctx);
            } else if ft.is_file() {
                let fname = path.file_name().unwrap().to_owned();

                if fname.contains(".") {
                    continue;
                }

                ctx.total += 1;

                let kn = fname.as_str().try_into().unwrap();
                match ctx.index.cache().read_cache_file(kn, &l) {
                    Ok(Some(e)) => {
                        if let Err(err) = e.deserialize() {
                            ctx.failed.insert(fname, err.to_string());
                        }
                    }
                    Ok(None) => {
                        ctx.failed.insert(fname, "not found".into());
                    }
                    Err(err) => {
                        ctx.failed.insert(fname, err.to_string());
                    }
                }
            }
        }
    }

    let mut ctx = Context {
        index: cio,
        total: 0,
        failed: Default::default(),
    };

    walk_dir(&root, &mut ctx);

    if !ctx.failed.is_empty() {
        panic!("{:#?}", ctx.failed);
    }
}
