#![cfg(all(feature = "local-builder", feature = "sparse"))]

mod utils;

use rayon::prelude::{IndexedParallelIterator, IntoParallelIterator, ParallelIterator};
use tame_index::index::local;

#[test]
fn builds_local_registry() {
    let sparse = tame_index::index::RemoteSparseIndex::new(
        tame_index::SparseIndex::new(tame_index::IndexLocation::new(
            tame_index::IndexUrl::CratesIoSparse,
        ))
        .unwrap(),
        reqwest::blocking::Client::new(),
    );

    let mut mdc = cargo_metadata::MetadataCommand::new();
    mdc.features(cargo_metadata::CargoOpt::AllFeatures);
    mdc.manifest_path("Cargo.toml");
    let md = mdc.exec().expect("failed to gather metadata");

    let mut krates = std::collections::BTreeMap::new();

    struct IndexPkg {
        ik: tame_index::IndexKrate,
        versions: Vec<smol_str::SmolStr>,
    }

    let lock = tame_index::utils::flock::FileLock::unlocked();

    for pkg in &md.packages {
        if pkg.name == "tame-index" {
            continue;
        }
        let ip = krates.entry(pkg.name.clone()).or_insert_with(|| {
            let ik = sparse
                .cached_krate(pkg.name.as_str().try_into().unwrap(), &lock)
                .map_err(|e| {
                    panic!("failed to read cache entry for {}: {e}", pkg.name);
                })
                .unwrap()
                .ok_or_else(|| {
                    panic!("no cache entry for {}", pkg.name);
                })
                .unwrap();

            IndexPkg {
                ik,
                versions: Vec::new(),
            }
        });

        ip.versions.push(pkg.version.to_string().into());
    }

    let client = local::builder::Client::build(reqwest::blocking::ClientBuilder::new()).unwrap();

    let lrb_td = utils::tempdir();
    let lrb = local::LocalRegistryBuilder::create(lrb_td.path().to_owned()).unwrap();
    let config = sparse.index.index_config().unwrap();

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
                local::ValidKrate::download(&client, &config, iv).unwrap()
            })
            .collect_into_vec(&mut crate_files);

        lrb.insert(&ipkg.ik, &crate_files).unwrap();
    });

    let _lr = lrb.finalize(true).unwrap();

    // Create a fake project and override the crates.io registry to point to
    // the local one we just created, it should get the same metadata
    let fake_project = utils::tempdir();

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
    mdc.manifest_path("Cargo.toml");
    // We need to override this otherwise our config won't be picked up
    mdc.current_dir(fake_project.path());

    // This will fail if we've missed one or more versions in the local registry
    mdc.exec().expect("failed to gather metadata");
}

/// Validates we get the correct checksum for a crate
#[test]
fn downloads_and_verifies() {
    let client = reqwest::blocking::Client::builder()
        .no_gzip()
        .build()
        .unwrap();

    let res = client
        .get("https://static.crates.io/crates/wasm-bindgen/wasm-bindgen-0.2.87.crate")
        .send()
        .unwrap()
        .error_for_status()
        .unwrap();

    let body = res.bytes().unwrap();

    use bytes::Buf;
    assert!(local::validate_checksum::<{ 16 * 1024 }>(
        body.reader(),
        &("7706a72ab36d8cb1f80ffbf0e071533974a60d0a308d01a5d0375bf60499a342"
            .parse()
            .unwrap()),
    )
    .unwrap());
}
