#![cfg(target_pointer_width = "64")]

//! Note we only run these tests if _built_ for 64-bit, as the only test targets
//! this project cares about are 64 bit, and the issue is that, to match cargo
//! we need to calculate the hash of the index the same, the rub is that when
//! running a 32-bit target (eg. i686) on a 64-bit host where the cargo on the
//! host is built for the 64-bit target as well, the local directories won't match

mod utils;

use tame_index::{IndexCache, index::cache::ValidCacheEntry, utils::get_index_details};

/// Validates that we can parse the current version of cache entries emitted by
/// cargo by reading the cache entry that should be there for one of this crate's
/// dependencies. This test should only fail if you have built the crate, wiped
/// the crates.io cache, then run the test executable without using cargo. If
/// you do that, that is your fault.
#[test]
fn parses_current_cargo_cache() {
    let stable = tame_index::utils::cargo_version(None).unwrap() >= semver::Version::new(1, 85, 0);
    let (path, _url) = get_index_details(tame_index::CRATES_IO_HTTP_INDEX, None, stable).unwrap();
    let cache = IndexCache::at_path(path);
    let lock = &utils::unlocked();

    let cached = cache
        .read_cache_file("camino".try_into().unwrap(), lock)
        .expect("failed to read cache file")
        .expect("cache file not found");

    let vce = ValidCacheEntry::read(&cached)
        .expect("failed to parse cargo cache file, we might need to update this crate");

    assert!(
        matches!(
            vce.revision,
            tame_index::index::cache::Revision::Http(
                tame_index::index::sparse::HttpRevision::Etag(_)
            )
        ),
        "cargo rarely/never writes anything but etag revisions to the cache"
    );

    let camino = vce
        .to_krate(None)
        .expect("failed to parse cache entry")
        .unwrap();

    // The initial version of camino used by this crate, this version will _always_
    // exist in the index, even if it yanked in the future, and is otherwise immutable
    let camino_version = camino
        .versions
        .iter()
        .find(|iv| iv.version == "1.1.4")
        .expect("failed to find expected version");

    assert_eq!(
        "c530edf18f37068ac2d977409ed5cd50d53d73bc653c7647b48eb78976ac9ae2",
        camino_version.checksum.to_string()
    );
}

/// Validates we can write cache files the exact same as the current version of cargo
#[test]
fn serializes_current_cargo_cache() {
    let stable = tame_index::utils::cargo_version(None).unwrap() >= semver::Version::new(1, 85, 0);
    let (path, _url) = get_index_details(tame_index::CRATES_IO_HTTP_INDEX, None, stable).unwrap();
    let cache = IndexCache::at_path(path);
    let lock = &utils::unlocked();

    let cargos = cache
        .read_cache_file("camino".try_into().unwrap(), lock)
        .expect("failed to read cache file")
        .expect("cache file not found");

    let cargo_ce = ValidCacheEntry::read(&cargos)
        .expect("failed to parse cargo cache file, we might need to update this crate");

    let cargos_krate = cargo_ce.to_krate(None).unwrap().unwrap();

    let mut ours = Vec::new();
    cargos_krate
        .write_cache_entry(&mut ours, &cargo_ce.revision)
        .expect("failed to write");

    let our_ce = ValidCacheEntry::read(&ours).expect("failed to parse our cache file");

    assert_eq!(cargo_ce.revision, our_ce.revision);

    let mut cargos = tame_index::index::cache::split(cargo_ce.version_entries, 0);
    let mut ours = tame_index::index::cache::split(our_ce.version_entries, 0);

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
}
