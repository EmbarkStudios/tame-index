pub mod cache;
pub mod git;
#[cfg(feature = "git")]
pub(crate) mod git_remote;
#[cfg(feature = "combo-index")]
mod index;
pub mod sparse;
#[cfg(feature = "sparse")]
mod sparse_remote;

pub use cache::IndexCache;
pub use git::GitIndex;
#[cfg(feature = "git")]
pub use git_remote::RemoteGitIndex;
#[cfg(feature = "combo-index")]
pub use index::Index;
pub use sparse::SparseIndex;
#[cfg(feature = "sparse")]
pub use sparse_remote::{AsyncRemoteSparseIndex, RemoteSparseIndex};

/// Global configuration of an index, reflecting the [contents of config.json](https://doc.rust-lang.org/cargo/reference/registries.html#index-format).
#[derive(Eq, PartialEq, PartialOrd, Ord, Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct IndexConfig {
    /// Pattern for creating download URLs. See [`Self::download_url`].
    pub dl: String,
    /// Base URL for publishing, etc.
    pub api: Option<String>,
}

impl IndexConfig {
    /// Gets the download url for the specified crate version
    ///
    /// See <https://doc.rust-lang.org/cargo/reference/registries.html#index-format>
    /// for more info
    pub fn download_url(&self, name: crate::KrateName<'_>, version: &str) -> String {
        let mut dl = self.dl.clone();

        while let Some(start) = dl.find("{crate}") {
            dl.replace_range(start..start + 7, name.0);
        }

        while let Some(start) = dl.find("{version}") {
            dl.replace_range(start..start + 9, version);
        }

        if dl.contains("{prefix}") || dl.contains("{lowerprefix}") {
            let mut prefix = String::with_capacity(6);
            name.prefix(&mut prefix, '/');

            while let Some(start) = dl.find("{prefix}") {
                dl.replace_range(start..start + 8, &prefix);
            }

            if dl.contains("{lowerprefix}") {
                prefix.make_ascii_lowercase();

                while let Some(start) = dl.find("{lowerprefix}") {
                    dl.replace_range(start..start + 13, &prefix);
                }
            }
        }

        dl
    }
}
