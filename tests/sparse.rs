mod utils;

use http::header;
use tame_index::{IndexLocation, IndexUrl, SparseIndex};

#[inline]
fn crates_io(path: impl AsRef<tame_index::Path>) -> SparseIndex {
    SparseIndex::new(
        IndexLocation::new(IndexUrl::CratesIoSparse).with_root(Some(path.as_ref().to_owned())),
    )
    .unwrap()
}

/// Validates the we get a valid root and krate url
#[test]
fn opens_crates_io() {
    let index = crates_io(env!("CARGO_MANIFEST_DIR"));

    assert_eq!(index.url(), "https://index.crates.io/");
    assert_eq!(
        index.crate_url("autocfg".try_into().unwrap()),
        "https://index.crates.io/au/to/autocfg"
    );
}

/// Validates a request can be made for a crate that doesn't have a local cache entry
#[test]
fn make_request_without_cache() {
    let index = crates_io(env!("CARGO_MANIFEST_DIR"));

    let req = index
        .make_remote_request("serde".try_into().unwrap(), None)
        .unwrap();

    let hdrs = req.headers();
    // Ensure neither of the possible cache headers are set
    assert!(
        hdrs.get(header::IF_MODIFIED_SINCE).is_none() && hdrs.get(header::IF_NONE_MATCH).is_none()
    );

    assert_eq!(hdrs.get("cargo-protocol").unwrap(), "version=1");
    assert_eq!(hdrs.get(header::ACCEPT).unwrap(), "text/plain");
    assert_eq!(hdrs.get(header::ACCEPT_ENCODING).unwrap(), "gzip");
}

const ETAG: &str = "W/\"fa62f662c9aae1f21cab393950d4ae23\"";
const DATE: &str = "Thu, 22 Oct 2023 09:40:03 GMT";

/// Validates appropriate headers are set when a cache entry does exist
#[test]
fn make_request_with_cache() {
    let td = utils::tempdir();

    let index = crates_io(&td);

    {
        let etag_krate = utils::fake_krate("etag-krate", 2);
        index
            .cache()
            .write_to_cache(&etag_krate, &format!("{}: {ETAG}", header::ETAG))
            .unwrap();

        let req = index
            .make_remote_request("etag-krate".try_into().unwrap(), None)
            .unwrap();

        assert_eq!(req.headers().get(header::IF_NONE_MATCH).unwrap(), ETAG);
    }

    {
        let req = index
            .make_remote_request("etag-specified-krate".try_into().unwrap(), Some(ETAG))
            .unwrap();

        assert_eq!(req.headers().get(header::IF_NONE_MATCH).unwrap(), ETAG);
    }

    {
        let modified_krate = utils::fake_krate("modified-krate", 2);
        index
            .cache()
            .write_to_cache(
                &modified_krate,
                &format!("{}: {DATE}", header::LAST_MODIFIED),
            )
            .unwrap();

        let req = index
            .make_remote_request("modified-krate".try_into().unwrap(), None)
            .unwrap();

        assert_eq!(req.headers().get(header::IF_MODIFIED_SINCE).unwrap(), DATE);
    }
}

/// Validates we can parse a response where the local cache version is up to date
#[test]
fn parse_unmodified_response() {
    let td = utils::tempdir();
    let index = crates_io(&td);

    let etag_krate = utils::fake_krate("etag-krate", 2);
    index
        .cache()
        .write_to_cache(&etag_krate, &format!("{}: {ETAG}", header::ETAG))
        .unwrap();

    let response = http::Response::builder()
        .status(http::StatusCode::NOT_MODIFIED)
        .header(header::ETAG, ETAG)
        .body(Vec::new())
        .unwrap();

    let cached_krate = index
        .parse_remote_response("etag-krate".try_into().unwrap(), response, true)
        .unwrap()
        .expect("cached krate");

    assert_eq!(etag_krate, cached_krate);
}

/// Validates we can parse a modified response
#[test]
fn parse_modified_response() {
    let td = utils::tempdir();
    let index = crates_io(&td);

    {
        let etag_krate = utils::fake_krate("etag-krate", 3);
        let mut serialized = Vec::new();
        etag_krate.write_json_lines(&mut serialized).unwrap();

        let response = http::Response::builder()
            .status(http::StatusCode::OK)
            .header(header::ETAG, ETAG)
            .body(serialized)
            .unwrap();

        let new_krate = index
            .parse_remote_response("etag-krate".try_into().unwrap(), response, true)
            .unwrap()
            .expect("new response");

        assert_eq!(etag_krate, new_krate);

        let cached_krate = index
            .cache()
            .cached_krate(
                "etag-krate".try_into().unwrap(),
                Some(&format!("{}: {ETAG}", header::ETAG)),
            )
            .unwrap()
            .expect("cached krate");

        assert_eq!(etag_krate, cached_krate);
    }

    {
        let modified_krate = utils::fake_krate("modified-krate", 3);
        let mut serialized = Vec::new();
        modified_krate.write_json_lines(&mut serialized).unwrap();

        let response = http::Response::builder()
            .status(http::StatusCode::OK)
            .header(header::LAST_MODIFIED, DATE)
            .body(serialized)
            .unwrap();

        let new_krate = index
            .parse_remote_response("modified-krate".try_into().unwrap(), response, true)
            .unwrap()
            .expect("new response");

        assert_eq!(modified_krate, new_krate);

        let cached_krate = index
            .cache()
            .cached_krate(
                "modified-krate".try_into().unwrap(),
                Some(&format!("{}: {DATE}", header::LAST_MODIFIED)),
            )
            .unwrap()
            .expect("cached krate");

        assert_eq!(modified_krate, cached_krate);
    }
}

/// Ensure we can actually send a request to crates.io and parse the response
#[test]
#[cfg(feature = "sparse")]
fn end_to_end() {
    let td = utils::tempdir();
    let index = crates_io(&td);

    let client = reqwest::blocking::Client::builder().build().unwrap();

    let rsi = tame_index::index::RemoteSparseIndex::new(index, client);

    let spdx_krate = rsi
        .krate("spdx".try_into().unwrap(), true)
        .expect("failed to retrieve spdx")
        .expect("failed to find spdx");

    spdx_krate
        .versions
        .iter()
        .find(|iv| iv.version == "0.10.1")
        .expect("failed to find expected version");
}
