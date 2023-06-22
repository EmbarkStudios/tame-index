mod dedupe;

use crate::Error;
use dedupe::DedupeContext;
use semver::Version;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::{collections::HashMap, sync::Arc};

/// A single version of a crate (package) published to the index
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct IndexVersion {
    pub name: SmolStr,
    #[serde(rename = "vers")]
    pub version: Version,
    pub deps: Arc<[IndexDependency]>,
    pub features: Arc<HashMap<String, Vec<String>>>,
    /// It's wrapped in `Option<Box>` to reduce size of the struct when the field is unused (i.e. almost always)
    /// <https://rust-lang.github.io/rfcs/3143-cargo-weak-namespaced-features.html#index-changes>
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[allow(clippy::box_collection)]
    pub features2: Option<Box<HashMap<String, Vec<String>>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Box<SmolStr>>,
    #[serde(default)]
    pub rust_version: Option<SmolStr>,
    #[serde(rename = "cksum")]
    pub checksum: Chksum,
    #[serde(default)]
    pub yanked: bool,
}

impl IndexVersion {
    /// Dependencies for this version
    #[inline]
    pub fn dependencies(&self) -> &[IndexDependency] {
        &self.deps
    }

    /// Checksum of the package for this version
    ///
    /// SHA256 of the .crate file
    #[inline]
    pub fn checksum(&self) -> &[u8; 32] {
        &self.checksum.0
    }

    /// Explicit features this crate has. This list is not exhaustive,
    /// because any optional dependency becomes a feature automatically.
    ///
    /// `default` is a special feature name for implicitly enabled features.
    #[inline]
    pub fn features(&self) -> &HashMap<String, Vec<String>> {
        &self.features
    }

    /// Exclusivity flag. If this is a sys crate, it informs it
    /// conflicts with any other crate with the same links string.
    ///
    /// It does not involve linker or libraries in any way.
    #[inline]
    pub fn links(&self) -> Option<&str> {
        self.links.as_ref().map(|s| s.as_str())
    }

    /// Whether this version was [yanked](http://doc.crates.io/crates-io.html#cargo-yank) from the
    /// index
    #[inline]
    pub fn is_yanked(&self) -> bool {
        self.yanked
    }

    /// Required version of rust
    ///
    /// Corresponds to `package.rust-version`.
    ///
    /// Added in 2023 (see <https://github.com/rust-lang/crates.io/pull/6267>),
    /// can be `None` if published before then or if not set in the manifest.
    #[inline]
    pub fn rust_version(&self) -> Option<&str> {
        self.rust_version.as_deref()
    }

    /// Retrieves the URL this crate version's tarball can be downloaded from
    #[inline]
    pub fn download_url(&self, index: &crate::index::IndexConfig) -> Option<String> {
        Some(index.download_url(
            self.name.as_str().try_into().ok()?,
            &self.version.to_string(),
        ))
    }
}

/// A single dependency of a specific crate version
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
pub struct IndexDependency {
    /// Dependency's arbitrary nickname (it may be an alias). Use [`Self::crate_name`] for actual crate name.
    pub name: SmolStr,
    pub req: semver::VersionReq,
    /// Double indirection to remove size from this struct, since the features are rarely set
    pub features: Box<Box<[String]>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<Box<SmolStr>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<DependencyKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Box<SmolStr>>,
    pub optional: bool,
    pub default_features: bool,
}

impl IndexDependency {
    /// Features unconditionally enabled when using this dependency,
    /// in addition to [`Dependency::has_default_features`] and features enabled
    /// through the parent crate's feature list.
    #[inline]
    pub fn features(&self) -> &[String] {
        &self.features
    }

    /// If it's optional, it implies a feature of its [`Dependency::name`], and
    /// can be enabled through the parent crate's features.
    #[inline]
    pub fn is_optional(&self) -> bool {
        self.optional
    }

    /// If `true` (default), enable `default` feature of this dependency
    #[inline]
    pub fn has_default_features(&self) -> bool {
        self.default_features
    }

    /// This dependency is only used when compiling for this `cfg` expression
    #[inline]
    pub fn target(&self) -> Option<&str> {
        self.target.as_ref().map(|s| s.as_str())
    }

    /// The kind of the dependency
    #[inline]
    pub fn kind(&self) -> DependencyKind {
        self.kind.unwrap_or_default()
    }

    /// Set if dependency's crate name is different from the `name` (alias)
    #[inline]
    pub fn package(&self) -> Option<&str> {
        self.package.as_ref().map(|s| s.as_str())
    }

    /// Returns the name of the crate providing the dependency.
    /// This is equivalent to `name()` unless `self.package()`
    /// is not `None`, in which case it's equal to `self.package()`.
    ///
    /// Basically, you can define a dependency in your `Cargo.toml`
    /// like this:
    ///
    /// ```toml
    /// serde_lib = {version = "1", package = "serde"}
    /// ```
    ///
    /// ...which means that it uses the crate `serde` but imports
    /// it under the name `serde_lib`.
    #[inline]
    pub fn crate_name(&self) -> &str {
        match &self.package {
            Some(s) => s,
            None => &self.name,
        }
    }
}

/// Section in which this dependency was defined
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum DependencyKind {
    /// Used at run time
    #[default]
    Normal,
    /// Not fetched and not used, except for when used direclty in a workspace
    Dev,
    /// Used at build time, not available at run time
    Build,
}

/// A whole crate with all its versions
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct IndexKrate {
    /// All versions of the crate, sorted chronologically by when it was published
    pub versions: Vec<IndexVersion>,
}

impl IndexKrate {
    /// The highest version as per semantic versioning specification
    ///
    /// Note this may be a pre-release or yanked, use [`Self::highest_normal_version`]
    /// to filter to the highest version that is not one of those
    #[inline]
    pub fn highest_version(&self) -> &IndexVersion {
        self.versions
            .iter()
            .max_by_key(|v| &v.version)
            // Safety: Versions inside the index will always adhere to
            // semantic versioning. If a crate is inside the index, at
            // least one version is available.
            .unwrap()
    }

    /// Returns crate version with the highest version number according to semver,
    /// but excludes pre-release and yanked versions.
    ///
    /// 0.x.y versions are included.
    ///
    /// May return `None` if the crate has only pre-release or yanked versions.
    #[inline]
    pub fn highest_normal_version(&self) -> Option<&IndexVersion> {
        self.versions
            .iter()
            .filter(|v| !v.is_yanked() && v.version.pre.is_empty())
            .max_by_key(|v| &v.version)
    }

    /// The crate's unique registry name. Case-sensitive, mostly.
    #[inline]
    pub fn name(&self) -> &str {
        &self.versions[0].name
    }

    /// The last release by date, even if it's yanked or less than highest version.
    ///
    /// See [`Self::highest_normal_version`]
    #[inline]
    pub fn most_recent_version(&self) -> &IndexVersion {
        &self.versions[self.versions.len() - 1]
    }

    /// First version ever published. May be yanked.
    ///
    /// It is not guaranteed to be the lowest version number.
    #[inline]
    pub fn earliest_version(&self) -> &IndexVersion {
        &self.versions[0]
    }
}

impl IndexKrate {
    /// Parse an index file with all of crate's versions.
    ///
    /// The file must contain at least one version.
    #[inline]
    pub fn new(index_path: impl AsRef<crate::Path>) -> Result<Self, Error> {
        let lines = std::fs::read(index_path.as_ref())?;
        Self::from_slice(&lines)
    }

    /// Parse a crate from in-memory JSON-lines data
    #[inline]
    pub fn from_slice(bytes: &[u8]) -> Result<Self, Error> {
        let mut dedupe = DedupeContext::default();
        Self::from_slice_with_context(bytes, &mut dedupe)
    }

    /// Parse a [`Self`] file from in-memory JSON data
    pub(crate) fn from_slice_with_context(
        mut bytes: &[u8],
        dedupe: &mut DedupeContext,
    ) -> Result<Self, Error> {
        use crate::index::cache::split;
        // Trim last newline(s) so we don't need to special case the split
        while bytes.last() == Some(&b'\n') {
            bytes = &bytes[..bytes.len() - 1];
        }

        let num_versions = split(bytes, b'\n').count();
        let mut versions = Vec::with_capacity(num_versions);
        for line in split(bytes, b'\n') {
            let mut version: IndexVersion = serde_json::from_slice(line)?;

            if let Some(features2) = version.features2.take() {
                if let Some(f1) = Arc::get_mut(&mut version.features) {
                    for (key, mut val) in features2.into_iter() {
                        f1.entry(key).or_insert_with(Vec::new).append(&mut val);
                    }
                }
            }

            // Many versions have identical dependencies and features
            dedupe.deps(&mut version.deps);
            dedupe.features(&mut version.features);

            versions.push(version);
        }

        if versions.is_empty() {
            return Err(Error::NoCrateVersions);
        }

        Ok(Self { versions })
    }

    /// Writes this crate into a JSON-lines formatted buffer
    ///
    /// Note this creates its own internal [`std::io::BufWriter`], there is no
    /// need to wrap it in your own
    pub fn write_json_lines<W: std::io::Write>(&self, writer: &mut W) -> Result<(), Error> {
        use std::io::{BufWriter, Write};

        let mut w = BufWriter::new(writer);
        for iv in &self.versions {
            serde_json::to_writer(&mut w, &iv)?;
            w.write_all(b"\n")?;
        }

        Ok(w.flush()?)
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct Chksum(pub [u8; 32]);

use std::fmt;

impl fmt::Debug for Chksum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut hex = [0; 64];
        let hs = crate::utils::encode_hex(&self.0, &mut hex);

        f.debug_struct("Chksum").field("sha-256", &hs).finish()
    }
}

impl fmt::Display for Chksum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut hex = [0; 64];
        let hs = crate::utils::encode_hex(&self.0, &mut hex);

        f.write_str(hs)
    }
}

impl<'de> Deserialize<'de> for Chksum {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        struct HexStrVisitor;

        impl<'de> serde::de::Visitor<'de> for HexStrVisitor {
            type Value = Chksum;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "a hex encoded string")
            }

            fn visit_str<E: Error>(self, data: &str) -> Result<Self::Value, E> {
                if data.len() != 64 {
                    return Err(serde::de::Error::invalid_length(
                        data.len(),
                        &"a string with 64 characters",
                    ));
                }

                let mut array = [0u8; 32];

                for (ind, chunk) in data.as_bytes().chunks(2).enumerate() {
                    #[inline]
                    fn parse_hex<E: Error>(b: u8) -> Result<u8, E> {
                        Ok(match b {
                            b'A'..=b'F' => b - b'A' + 10,
                            b'a'..=b'f' => b - b'a' + 10,
                            b'0'..=b'9' => b - b'0',
                            c => {
                                return Err(Error::invalid_value(
                                    serde::de::Unexpected::Char(c as char),
                                    &"a hexadecimal character",
                                ))
                            }
                        })
                    }

                    let mut cur = parse_hex(chunk[0])?;
                    cur <<= 4;
                    cur |= parse_hex(chunk[1])?;

                    array[ind] = cur;
                }

                Ok(Chksum(array))
            }

            fn visit_borrowed_str<E: Error>(self, data: &'de str) -> Result<Self::Value, E> {
                self.visit_str(data)
            }
        }

        deserializer.deserialize_str(HexStrVisitor)
    }
}

impl Serialize for Chksum {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut raw = [0u8; 64];
        let s = crate::utils::encode_hex(&self.0, &mut raw);
        serializer.serialize_str(s)
    }
}
