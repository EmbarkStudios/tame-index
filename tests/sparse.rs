#![allow(missing_docs)]

mod utils;

use http::header;
use tame_index::{
    IndexLocation, IndexUrl, SparseIndex,
    index::{
        cache::Revision,
        sparse::{HttpRevision, Request},
    },
};

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

fn assert_request_headers(expected: &[(&str, &str)], req: &Request) {
    for (n, v) in expected {
        let Some(hv) = req
            .headers
            .iter()
            .find_map(|(hn, hv)| (hn.as_str() == *n).then_some(hv))
        else {
            panic!("failed to find '{n}'");
        };

        assert_eq!(v, hv, "{n}");
    }
}

/// Validates a request can be made for a crate that doesn't have a local cache entry
#[test]
fn make_request_without_cache() {
    let index = crates_io(env!("CARGO_MANIFEST_DIR"));
    let lock = &utils::unlocked();

    let req = index
        .make_remote_request("serde".try_into().unwrap(), None, lock)
        .unwrap();

    assert_request_headers(
        &[
            (header::IF_NONE_MATCH.as_str(), ""),
            ("cargo-protocol", "version=1"),
            (header::ACCEPT.as_str(), "text/plain"),
        ],
        &req,
    );
}

const ETAG: &str = "\"fa62f662c9aae1f21cab393950d4ae23\"";
const DATE: &str = "Thu, 22 Oct 2023 09:40:03 GMT";

/// Validates appropriate headers are set when a cache entry does exist
#[test]
fn make_request_with_cache() {
    let td = utils::tempdir();
    let index = crates_io(&td);
    let lock = &utils::unlocked();

    {
        let etag_krate = utils::fake_krate("etag-krate", 2);
        index
            .cache()
            .write_to_cache(
                &etag_krate,
                &Revision::Http(HttpRevision::Etag("fake")),
                lock,
            )
            .unwrap();

        let req = index
            .make_remote_request("etag-krate".try_into().unwrap(), None, lock)
            .unwrap();

        assert_request_headers(&[(header::IF_NONE_MATCH.as_str(), "fake")], &req);
    }

    {
        let req = index
            .make_remote_request(
                "etag-specified-krate".try_into().unwrap(),
                Some(HttpRevision::Etag(ETAG)),
                lock,
            )
            .unwrap();

        assert_request_headers(&[(header::IF_NONE_MATCH.as_str(), ETAG)], &req);
    }

    {
        let modified_krate = utils::fake_krate("modified-krate", 2);
        index
            .cache()
            .write_to_cache(
                &modified_krate,
                &Revision::Http(HttpRevision::LastModified(DATE)),
                lock,
            )
            .unwrap();

        let req = index
            .make_remote_request("modified-krate".try_into().unwrap(), None, lock)
            .unwrap();

        assert_request_headers(&[(header::IF_MODIFIED_SINCE.as_str(), DATE)], &req);
    }
}

/// Validates we can parse a response where the local cache version is up to date
#[test]
fn parse_unmodified_response() {
    let td = utils::tempdir();
    let index = crates_io(&td);
    let lock = &utils::unlocked();

    let etag_krate = utils::fake_krate("etag-krate", 2);
    index
        .cache()
        .write_to_cache(&etag_krate, &Revision::Http(HttpRevision::Etag(ETAG)), lock)
        .unwrap();

    let response = http::Response::builder()
        .status(http::StatusCode::NOT_MODIFIED)
        .header(header::ETAG, ETAG)
        .body::<&[u8]>(&[])
        .unwrap();

    let cached_krate = index
        .parse_remote_response(
            "etag-krate".try_into().unwrap(),
            (&response).into(),
            true,
            lock,
        )
        .unwrap()
        .expect("cached krate");

    assert_eq!(etag_krate, cached_krate);
}

/// Validates we can parse a modified response
#[test]
fn parse_modified_response() {
    let td = utils::tempdir();
    let index = crates_io(&td);
    let lock = &utils::unlocked();

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
            .parse_remote_response(
                "etag-krate".try_into().unwrap(),
                (&response).into(),
                true,
                lock,
            )
            .unwrap()
            .expect("new response");

        assert_eq!(etag_krate, new_krate);

        let cached_krate = index
            .cache()
            .cached_krate(
                "etag-krate".try_into().unwrap(),
                Some(Revision::Http(HttpRevision::Etag(ETAG))),
                lock,
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
            .parse_remote_response(
                "modified-krate".try_into().unwrap(),
                (&response).into(),
                true,
                lock,
            )
            .unwrap()
            .expect("new response");

        assert_eq!(modified_krate, new_krate);

        let cached_krate = index
            .cache()
            .cached_krate(
                "modified-krate".try_into().unwrap(),
                Some(Revision::Http(HttpRevision::LastModified(DATE))),
                lock,
            )
            .unwrap()
            .expect("cached krate");

        assert_eq!(modified_krate, cached_krate);
    }
}
