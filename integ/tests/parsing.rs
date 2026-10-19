#[test]
fn walks_entries() {
    const EXTRA_SPACES: &[u8] = include_bytes!("extra_spaces.json");
    let lock = tame_index::utils::flock::FileLock::unlocked();

    for ie in [
        tame_index::index::IndexEntry::Local(
            tame_index::index::LockedMmap::mmap(
                tame_index::Path::new(&format!("{}/tests/normal.json", env!("CARGO_MANIFEST_DIR"))),
                &lock,
            )
            .unwrap(),
        ),
        tame_index::index::IndexEntry::Cache(
            tame_index::index::LockedMmap::mmap(
                tame_index::Path::new(&format!(
                    "{}/tests/cache_git.bin",
                    env!("CARGO_MANIFEST_DIR")
                )),
                &lock,
            )
            .unwrap(),
        ),
        tame_index::index::IndexEntry::Remote(
            tame_index::index::JsonLines::validate(EXTRA_SPACES).unwrap(),
        ),
    ] {
        let ik = ie.deserialize().unwrap();

        for (i, entry) in ie.iter().unwrap().enumerate() {
            let expected = &ik.versions[i];
            {
                let ent = entry.deserialize().unwrap();
                assert_eq!(&ent, expected);
            }

            {
                let (rawv, json) = entry.raw();
                assert_eq!(rawv, expected.version.to_string());
                let des: tame_index::IndexVersion = serde_json::from_slice(json).unwrap();
                assert_eq!(&des, expected);
            }

            {
                let (vers, is_yanked) = entry.yanked().unwrap();
                assert_eq!(vers, expected.version.parse().unwrap());
                assert_eq!(is_yanked, expected.yanked);
            }
        }
    }
}
