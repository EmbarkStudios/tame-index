use rayon::prelude::{IndexedParallelIterator, IntoParallelIterator, ParallelIterator};
use tame_index::{external::http, index::local::builder};

fn unwrap_body(res: http::Response<ureq::Body>) -> tame_index::external::bytes::Bytes {
    assert!(res.status().is_success());
    res.into_body().read_to_vec().unwrap().into()
}

fn download<'iv>(
    client: &ureq::Agent,
    config: &tame_index::index::IndexConfig,
    version: &'iv tame_index::IndexVersion,
) -> builder::ValidKrate<'iv> {
    let url = config.download_url(
        version.name.as_str().try_into().unwrap(),
        version.version.as_ref(),
    );

    let body = unwrap_body(client.get(url).call().unwrap());
    builder::ValidKrate::validate(body, version).unwrap()
}

#[test]
#[ignore = "this can take up to 5s to complete"]
fn builds_local_registry() {
    let sparse = tame_index::SparseIndex::new(tame_index::IndexLocation::new(
        tame_index::IndexUrl::CratesIoSparse,
    ))
    .unwrap();

    let mut mdc = cargo_metadata::MetadataCommand::new();
    mdc.features(cargo_metadata::CargoOpt::AllFeatures);

    let mp = {
        let s = String::from_utf8(
            std::process::Command::new("git")
                .args(["rev-parse", "--show-toplevel"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();

        let mut s = s.trim().to_owned();
        s.push_str("/Cargo.toml");
        s
    };

    mdc.manifest_path(mp.clone());
    let md = mdc.exec().expect("failed to gather metadata");

    let mut krates = std::collections::BTreeMap::new();

    struct IndexPkg {
        ik: tame_index::IndexKrate,
        versions: Vec<smol_str::SmolStr>,
    }

    let lock = &integ::unlocked();

    for pkg in &md.packages {
        if pkg.name.as_ref() == "tame-index" {
            continue;
        }

        let ip = krates.entry(pkg.name.clone()).or_insert_with(|| {
            let ik = sparse
                .cached_krate(pkg.name.as_str().try_into().unwrap(), lock)
                .map_err(|e| {
                    panic!("failed to read cache entry for {}: {e}", pkg.name);
                })
                .unwrap()
                .ok_or_else(|| {
                    panic!("no cache entry for {}", pkg.name);
                })
                .unwrap();

            IndexPkg {
                ik: ik.deserialize().unwrap(),
                versions: Vec::new(),
            }
        });

        ip.versions.push(pkg.version.to_string().into());
    }

    let client = ureq::agent();

    let lrb_td = integ::tempdir!();
    let lrb = builder::LocalRegistryBuilder::create(lrb_td.path().to_owned()).unwrap();
    let config = sparse.index_config().unwrap();

    krates.into_par_iter().for_each(|(_krate, ipkg)| {
        let mut crate_files = Vec::with_capacity(ipkg.versions.len());

        ipkg.versions
            .into_par_iter()
            .map(|vers| {
                let iv = ipkg
                    .ik
                    .versions
                    .iter()
                    .find(|iv| iv.version == vers)
                    .unwrap();
                download(&client, &config, iv)
            })
            .collect_into_vec(&mut crate_files);

        lrb.insert(&ipkg.ik, &crate_files).unwrap();
    });

    let _lr = lrb.finalize(true).unwrap();

    // Create a fake project and override the crates.io registry to point to
    // the local one we just created, it should get the same metadata
    let fake_project = integ::tempdir!();

    std::fs::copy("Cargo.toml", fake_project.path().join("Cargo.toml")).unwrap();
    std::fs::copy("Cargo.lock", fake_project.path().join("Cargo.lock")).unwrap();

    let mut config = fake_project.path().join(".cargo");
    std::fs::create_dir(&config).unwrap();

    config.push("config.toml");

    // Windows is terrible
    let local_path = lrb_td.path().as_str();
    let local_path = local_path.replace('\\', "/");

    std::fs::write(
        &config,
        format!(
            r#"
[source.crates-io]
replace-with = "test-registry"

[source.test-registry]
local-registry = "{local_path}""#,
        ),
    )
    .unwrap();

    // We also need to create a fake lib.rs otherwise cargo will be sad
    {
        let librs = fake_project.path().join("src/lib.rs");
        std::fs::create_dir_all(librs.parent().unwrap()).unwrap();
        std::fs::write(librs, "").unwrap();
    }

    let mut mdc = cargo_metadata::MetadataCommand::new();
    mdc.features(cargo_metadata::CargoOpt::AllFeatures);
    mdc.manifest_path(mp);
    // We need to override this otherwise our config won't be picked up
    mdc.current_dir(fake_project.path());

    // This will fail if we've missed one or more versions in the local registry
    mdc.exec().expect("failed to gather metadata");
}

/// Validates we get the correct checksum for a crate
#[test]
fn downloads_and_verifies() {
    let res = ureq::get("https://static.crates.io/crates/wasm-bindgen/wasm-bindgen-0.2.87.crate")
        .call()
        .unwrap();

    let body = unwrap_body(res);

    use tame_index::external::bytes::Buf;
    assert!(
        builder::validate_checksum::<{ 16 * 1024 }>(
            body.reader(),
            &("7706a72ab36d8cb1f80ffbf0e071533974a60d0a308d01a5d0375bf60499a342"
                .parse()
                .unwrap()),
        )
        .unwrap()
    );
}

#[test]
fn finds_source_replacement() {
    let td = tempfile::tempdir().unwrap();
    let cfg_toml = td.path().join(".cargo/config.toml");
    std::fs::create_dir_all(cfg_toml.parent().unwrap()).unwrap();
    std::fs::write(
        &cfg_toml,
        r#"
[registries.imaginary-registry]
credential-provider = ["cargo:token"]
index = "sparse+https://imaginary-registry.example.net/index/"

[source._real_cargo_ignores_this_]
registry = "sparse+https://imaginary-registry.example.net/index/"
replace-with = "actual-registry-on-disk"

[source.actual-registry-on-disk]
local-registry = "test-local-registry"
"#,
    )
    .unwrap();

    let meta_path = td
        .path()
        .join("test-local-registry/index/ex/am/example-dependency");
    std::fs::create_dir_all(meta_path.parent().unwrap()).unwrap();
    std::fs::write(&meta_path, r#"{"name":"example-dependency","vers":"2.0.0","deps":[],"cksum":"a89f6827d6042043aa77282b091280d2ebb47365e3ff097d07f45b5a797dff63","features":{},"yanked":false}"#).unwrap();

    let subdir = td.path().join("test-subdirectory");
    std::fs::create_dir_all(&subdir).unwrap();

    let url = tame_index::IndexUrl::for_registry_name(
        Some(subdir.try_into().unwrap()),
        None,
        "imaginary-registry",
    )
    .unwrap();

    assert!(matches!(url, tame_index::IndexUrl::Local(_)));
    let index =
        tame_index::index::ComboIndexCache::new(tame_index::IndexLocation::new(url)).unwrap();
    let lock = tame_index::flock::FileLock::unlocked();
    let krate = index
        .cached_krate(integ::kn!("example-dependency"), &lock)
        .unwrap()
        .unwrap();
    assert_eq!(krate.iter().unwrap().next().unwrap().raw().0, "2.0.0");
}
