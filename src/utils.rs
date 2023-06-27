use crate::{Error, InvalidUrl, InvalidUrlError, PathBuf};

#[inline]
pub fn cargo_home() -> Result<crate::PathBuf, crate::Error> {
    Ok(crate::PathBuf::from_path_buf(home::cargo_home()?)?)
}

/// Encodes a slice of bytes into a hexadecimal string to the specified buffer
pub(crate) fn encode_hex<'out, const I: usize, const O: usize>(
    input: &[u8; I],
    output: &'out mut [u8; O],
) -> &'out str {
    assert_eq!(I * 2, O);

    const CHARS: &[u8] = b"0123456789abcdef";

    for (i, &byte) in input.iter().enumerate() {
        let i = i * 2;
        output[i] = CHARS[(byte >> 4) as usize];
        output[i + 1] = CHARS[(byte & 0xf) as usize];
    }

    // SAFETY: we only emit ASCII hex characters
    #[allow(unsafe_code)]
    unsafe {
        std::str::from_utf8_unchecked(output)
    }
}

/// Converts a full index url into a relative path and its canonical form
///
/// Cargo uses a small algorithm to create unique directory names for any url
/// so that they can be located in the same root without clashing
pub fn url_to_local_dir(url: &str) -> Result<(String, String), Error> {
    #[allow(deprecated)]
    fn hash_u64(url: &str, registry_kind: u64) -> u64 {
        use std::hash::{Hash, Hasher, SipHasher};

        let mut hasher = SipHasher::new_with_keys(0, 0);
        // Registry
        registry_kind.hash(&mut hasher);
        // Url
        url.hash(&mut hasher);
        hasher.finish()
    }

    const GIT_REGISTRY: u64 = 2;
    const SPARSE_REGISTRY: u64 = 3;

    let mut registry_kind = GIT_REGISTRY;

    // Ensure we have a registry or bare url
    let (url, scheme_ind) = {
        let scheme_ind = url.find("://").ok_or_else(|| InvalidUrl {
            url: url.to_owned(),
            source: InvalidUrlError::MissingScheme,
        })?;

        let scheme_str = &url[..scheme_ind];
        if scheme_str.starts_with("sparse+http") {
            registry_kind = SPARSE_REGISTRY;
            (url, scheme_ind)
        } else if let Some(ind) = scheme_str.find('+') {
            if &scheme_str[..ind] != "registry" {
                return Err(InvalidUrl {
                    url: url.to_owned(),
                    source: InvalidUrlError::UnknownSchemeModifier,
                }
                .into());
            }

            (&url[ind + 1..], scheme_ind - ind - 1)
        } else {
            (url, scheme_ind)
        }
    };

    // Could use the Url crate for this, but it's simple enough and we don't
    // need to deal with every possible url (I hope...)
    let host = match url[scheme_ind + 3..].find('/') {
        Some(end) => &url[scheme_ind + 3..scheme_ind + 3 + end],
        None => &url[scheme_ind + 3..],
    };

    // trim port
    let host = host.split(':').next().unwrap();

    let make_ident = |url: &str| -> String {
        let hash = hash_u64(url, registry_kind);
        let mut raw_ident = [0u8; 16];
        let ident = encode_hex(&hash.to_le_bytes(), &mut raw_ident);

        format!("{host}-{ident}")
    };

    let (ident, url) = if registry_kind == 2 {
        // cargo special cases github.com for reasons, so do the same
        let mut canonical = if host == "github.com" {
            url.to_lowercase()
        } else {
            url.to_owned()
        };

        // Chop off any query params/fragments
        if let Some(hash) = canonical.rfind('#') {
            canonical.truncate(hash);
        }

        if let Some(query) = canonical.rfind('?') {
            canonical.truncate(query);
        }

        let ident = make_ident(&canonical);

        if canonical.ends_with('/') {
            canonical.pop();
        }

        if canonical.contains("github.com/") && canonical.ends_with(".git") {
            // Only GitHub (crates.io) repositories have their .git suffix truncated
            canonical.truncate(canonical.len() - 4);
        }

        (ident, canonical)
    } else {
        (make_ident(url), url.to_owned())
    };

    Ok((ident, url))
}

/// Get the disk location of the specified url, as well as its canonical form
///
/// If not specified, the root directory is the user's default cargo home
pub fn get_index_details(url: &str, root: Option<PathBuf>) -> Result<(PathBuf, String), Error> {
    let (dir_name, canonical_url) = url_to_local_dir(url)?;

    let mut path = match root {
        Some(path) => path,
        None => cargo_home()?,
    };

    path.push("registry");
    path.push("index");
    path.push(dir_name);

    Ok((path, canonical_url))
}

#[cfg(test)]
mod test {
    use super::get_index_details;
    use crate::PathBuf;

    #[test]
    fn matches_cargo() {
        assert_eq!(
            get_index_details(crate::CRATES_IO_INDEX, Some(PathBuf::new())).unwrap(),
            (
                "registry/index/github.com-1ecc6299db9ec823".into(),
                crate::CRATES_IO_INDEX.to_owned()
            )
        );

        assert_eq!(
            get_index_details(crate::CRATES_IO_HTTP_INDEX, Some(PathBuf::new())).unwrap(),
            (
                "registry/index/index.crates.io-6f17d22bba15001f".into(),
                crate::CRATES_IO_HTTP_INDEX.to_owned(),
            )
        );

        // I've confirmed this also works with a custom registry, unfortunately
        // that one includes a secret key as part of the url which would allow
        // anyone to publish to the registry, so uhh...here's a fake one instead
        assert_eq!(
            get_index_details(
                "https://dl.cloudsmith.io/aBcW1234aBcW1234/embark/rust/cargo/index.git",
                Some(PathBuf::new())
            )
            .unwrap(),
            (
                "registry/index/dl.cloudsmith.io-ff79e51ddd2b38fd".into(),
                "https://dl.cloudsmith.io/aBcW1234aBcW1234/embark/rust/cargo/index.git".to_owned()
            )
        );

        // Ensure we actually strip off the irrelevant parts of a url, note that
        // the .git suffix is not part of the canonical url, but *is* used when hashing
        assert_eq!(
            get_index_details(
                &format!(
                    "registry+{}.git?one=1&two=2#fragment",
                    crate::CRATES_IO_INDEX
                ),
                Some(PathBuf::new())
            )
            .unwrap(),
            (
                "registry/index/github.com-c786010fb7ef2e6e".into(),
                crate::CRATES_IO_INDEX.to_owned()
            )
        );
    }
}
