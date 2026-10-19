use tame_index::IndexUrl;

/// Verifies we can parse .cargo/config.toml files to either use the crates-io
/// protocol set, or source replacements
#[test]
fn parses_from_file() {
    assert!(std::env::var_os("CARGO_REGISTRIES_CRATES_IO_PROTOCOL").is_none());

    let td = integ::tempdir!();
    let root = td.path().to_owned();
    let cfg_toml = td.path().join(".cargo/config.toml");

    std::fs::create_dir_all(cfg_toml.parent().unwrap()).unwrap();

    const GIT: &str = r#"[registries.crates-io]
protocol = "git"
"#;

    // First just set the protocol from the sparse default to git
    std::fs::write(&cfg_toml, GIT).unwrap();

    let iurl = IndexUrl::crates_io(Some(root.clone()), None, None).unwrap();
    assert_eq!(iurl.as_str(), tame_index::CRATES_IO_INDEX);
    assert!(!iurl.is_sparse());

    // Next set replacement registries
    for (i, (kind, url)) in [
        (
            "registry",
            "sparse+https://sparse-registry-parses-from-file.com",
        ),
        ("registry", "https://sparse-registry-parses-from-file.git"),
        ("local-registry", root.as_str()),
    ]
    .iter()
    .enumerate()
    {
        std::fs::write(&cfg_toml, format!("{GIT}\n[source.crates-io]\nreplace-with = 'replacement'\n[source.replacement]\n{kind} = '{url}'")).unwrap();

        let iurl = IndexUrl::crates_io(Some(root.clone()), None, None).unwrap();
        assert_eq!(i == 0, iurl.is_sparse());
        assert_eq!(iurl.as_str(), *url);
    }
}

#[test]
#[allow(unsafe_code)]
fn custom() {
    assert!(std::env::var_os("CARGO_REGISTRIES_TAME_INDEX_TEST_INDEX").is_none());

    let td = integ::tempdir!();
    let root = td.path().to_owned();
    let cfg_toml = td.path().join(".cargo/config.toml");

    std::fs::create_dir_all(cfg_toml.parent().unwrap()).unwrap();

    const SPARSE: &str = r#"[registries.tame-index-test]
index = "sparse+https://some-url.com"
"#;

    const GIT: &str = r#"[registries.tame-index-test]
    index = "https://some-url.com"
    "#;

    unsafe {
        std::fs::write(&cfg_toml, SPARSE).unwrap();

        let iurl =
            IndexUrl::for_registry_name(Some(root.clone()), None, "tame-index-test").unwrap();
        assert_eq!(iurl.as_str(), "sparse+https://some-url.com");
        assert!(iurl.is_sparse());

        std::env::set_var(
            "CARGO_REGISTRIES_TAME_INDEX_TEST_INDEX",
            "sparse+https://some-other-url.com",
        );

        let iurl =
            IndexUrl::for_registry_name(Some(root.clone()), None, "tame-index-test").unwrap();
        assert_eq!(iurl.as_str(), "sparse+https://some-other-url.com");
        assert!(iurl.is_sparse());

        std::env::remove_var("CARGO_REGISTRIES_TAME_INDEX_TEST_INDEX");
    }

    unsafe {
        std::fs::write(&cfg_toml, GIT).unwrap();

        let iurl =
            IndexUrl::for_registry_name(Some(root.clone()), None, "tame-index-test").unwrap();
        assert_eq!(iurl.as_str(), "https://some-url.com");
        assert!(!iurl.is_sparse());

        std::env::set_var(
            "CARGO_REGISTRIES_TAME_INDEX_TEST_INDEX",
            "https://some-other-url.com",
        );

        let iurl =
            IndexUrl::for_registry_name(Some(root.clone()), None, "tame-index-test").unwrap();
        assert_eq!(iurl.as_str(), "https://some-other-url.com");
        assert!(!iurl.is_sparse());

        std::env::remove_var("CARGO_REGISTRIES_TAME_INDEX_TEST_INDEX");
    }

    #[allow(unused_variables)]
    {
        let err = tame_index::Error::UnknownRegistry("non-existant".to_owned());
        assert!(matches!(
            IndexUrl::for_registry_name(Some(root.clone()), None, "non-existant"),
            Err(err),
        ));
    }
}
