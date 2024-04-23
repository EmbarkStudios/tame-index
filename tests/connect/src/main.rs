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

    let proc_count: usize = if std::env::var_os("CI").is_none() {
        // The connection count should be roughly the same as the processor count
        let stdout = std::process::Command::new("nproc").output().unwrap().stdout;

        std::str::from_utf8(&stdout)
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    } else {
        30
    };

    let max = proc_count + (proc_count as f32 * 0.05).floor() as usize;

    for test in ["reuses_connection", "async_reuses_connection"] {
        let path = format!("/tmp/tame-index-connection-trace-{test}");
        assert!(
            std::process::Command::new("strace")
                .args(["-f", "-e", "trace=connect", "-o", &path,])
                .arg(&latest)
                .arg("--exact")
                .arg(format!("remote::{test}"))
                .status()
                .unwrap()
                .success(),
            "failed to strace test"
        );

        let trace = std::fs::read_to_string(path).expect("failed to read strace output");

        let connection_counts = trace
            .lines()
            .filter(|line| line.contains("connect("))
            .count();

        if std::env::var_os("CI").is_some() {
            println!("{trace}");
        }

        assert!(
            connection_counts <= max,
            "connection syscalls ({connection_counts}) should be lower than {max}"
        );
    }
}
