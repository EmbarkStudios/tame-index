#![allow(dead_code)]

use std::sync::Arc;
pub use tame_index::{IndexKrate, PathBuf};

pub struct TempDir {
    td: tempfile::TempDir,
}

impl TempDir {
    #[inline]
    pub fn new() -> Self {
        Self {
            td: tempfile::TempDir::new_in(env!("CARGO_TARGET_TMPDIR")).unwrap(),
        }
    }
}

impl<'td> Into<PathBuf> for &'td TempDir {
    fn into(self) -> PathBuf {
        PathBuf::from_path_buf(self.td.path().to_owned()).unwrap()
    }
}

pub fn fake_krate(name: &str, num_versions: u8) -> IndexKrate {
    assert!(num_versions > 0);
    let mut version = semver::Version::new(0, 0, 0);
    let mut versions = Vec::new();

    for v in 0..num_versions {
        match v % 3 {
            0 => version.patch += 1,
            1 => {
                version.patch = 0;
                version.minor += 1;
            }
            2 => {
                version.patch = 0;
                version.minor = 0;
                version.major += 1;
            }
            _ => unreachable!(),
        }

        let iv = tame_index::IndexVersion {
            name: name.into(),
            version: version.clone(),
            deps: Arc::new([]),
            features: Arc::default(),
            features2: None,
            links: None,
            rust_version: None,
            checksum: tame_index::krate::Chksum(Default::default()),
            yanked: false,
        };

        versions.push(iv);
    }

    IndexKrate { versions }
}
