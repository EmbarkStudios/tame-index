//! Runs the sparse reuses_connection test to ensure we aren't creating too many
//! connections as a repro for https://github.com/EmbarkStudios/tame-index/issues/46

fn main() {
    // Build the binary so we know it's up to date, and so we can use the one
    // last modified in the target directory
    assert!(
        std::process::Command::new("cargo")
            .args([
                "test",
                "--no-run",
                "--test",
                "sparse",
                "--features",
                "sparse"
            ])
            .status()
            .unwrap()
            .success(),
        "failed to build test binary"
    );

    let mut latest = std::path::PathBuf::new();
    let mut ts = std::time::SystemTime::UNIX_EPOCH;

    for entry in std::fs::read_dir("target/debug/deps").expect("failed to read deps") {
        let entry = entry.expect("failed to read entry");

        if !entry
            .file_name()
            .as_os_str()
            .to_str()
            .unwrap()
            .starts_with("sparse-")
        {
            continue;
        }

        let md = entry.metadata().expect("failed to get metadata");

        let mt = md.modified().expect("failed to get mod time");

        if mt < ts {
            continue;
        }

        latest = entry.path();
        ts = mt;
    }

    // At this moment, crates.io should resolve to 4 IPv4 addresses, so we expect
    // those 4, as well as the TLS connection

    for test in ["reuses_connection", "async_reuses_connection"] {
        let path = format!("/tmp/tame-index-connection-trace-{test}");
        assert!(
            std::process::Command::new("strace")
                .args(["-f", "-e", "trace=connect", "-o", &path])
                .arg(&latest)
                .arg("--exact")
                .arg(format!("remote::{test}"))
                .arg("--nocapture")
                .env("RUST_LOG", "debug")
                .status()
                .unwrap()
                .success(),
            "failed to strace test"
        );

        let trace = std::fs::read_to_string(path).expect("failed to read strace output");

        let (connect_count, tls_count) = trace
            .lines()
            .filter_map(|line| {
                if !line.contains("connect(") {
                    return None;
                }

                if !line.contains("sa_family=AF_INET") {
                    return None;
                }

                if line.contains("sin_port=htons(0)") && line.ends_with(" = 0") {
                    Some((1, 0))
                } else if line.contains("sin_port=htons(443)")
                    && line.ends_with(" = -1 EINPROGRESS (Operation now in progress)")
                {
                    Some((0, 1))
                } else {
                    None
                }
            })
            .fold((0, 0), |acc, i| (acc.0 + i.0, acc.1 + i.1));

        if connect_count != 4 || tls_count != 1 {
            if std::env::var_os("CI").is_some() {
                eprintln!("{trace}");
            }
            panic!("should have established 4 (but got {connect_count}) connections and 1 (but got {tls_count}) TLS connection");
        }
    }
}
