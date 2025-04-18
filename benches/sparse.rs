//! Shutup clippy

type KrateSet = std::collections::BTreeSet<String>;

fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _rt = rt.enter();

    let bdir = tempfile::tempdir().unwrap();
    let adir = tempfile::tempdir().unwrap();

    // The setup of the indices is the same, and is not interesting to measure
    let bindex = {
        let loc = tame_index::IndexLocation {
            url: tame_index::IndexUrl::CratesIoSparse,
            root: tame_index::IndexPath::Exact(bdir.path().to_owned().try_into().unwrap()),
            cargo_version: Some(semver::Version::new(1, 85, 0)),
        };

        tame_index::index::RemoteSparseIndex::new(
            tame_index::SparseIndex::new(loc).unwrap(),
            tame_index::external::reqwest::blocking::ClientBuilder::new()
                .build()
                .unwrap(),
        )
    };

    let aindex = {
        let loc = tame_index::IndexLocation {
            url: tame_index::IndexUrl::CratesIoSparse,
            root: tame_index::IndexPath::Exact(adir.path().to_owned().try_into().unwrap()),
            cargo_version: Some(semver::Version::new(1, 85, 0)),
        };

        tame_index::index::AsyncRemoteSparseIndex::new(
            tame_index::SparseIndex::new(loc).unwrap(),
            tame_index::external::reqwest::ClientBuilder::new()
                .build()
                .unwrap(),
        )
    };

    let ks: KrateSet = KRATES.iter().map(|s| (*s).to_owned()).collect();

    let label = "sparse_fetch";
    let cfg = tiny_bench::BenchmarkConfig {
        num_samples: 10,
        ..Default::default()
    };
    tiny_bench::bench_with_setup_configuration_labeled(
        label,
        &cfg,
        || std::fs::remove_dir_all(bdir.path()),
        |_| blocking(&bindex, &ks),
    );
    tiny_bench::bench_with_setup_configuration_labeled(
        label,
        &cfg,
        || std::fs::remove_dir_all(adir.path()),
        |_| asunc(&aindex, &ks),
    );
}

fn blocking(rsi: &tame_index::index::RemoteSparseIndex, krates: &KrateSet) {
    let krates = rsi.krates(
        krates.clone(),
        true,
        &tame_index::index::FileLock::unlocked(),
    );

    for (krate, res) in krates {
        if let Err(err) = res {
            panic!("failed to download '{krate}': {err}");
        }
    }
}

fn asunc(rsi: &tame_index::index::AsyncRemoteSparseIndex, krates: &KrateSet) {
    let krates = rsi
        .krates_blocking(
            krates.clone(),
            true,
            None,
            &tame_index::index::FileLock::unlocked(),
        )
        .unwrap();

    for (krate, res) in krates {
        if let Err(err) = res {
            panic!("failed to download '{krate}': {err}");
        }
    }
}

/// The krates we want to sync. This is a "large" number to actually surpass
/// the core count of whatever machine the benchmarks are run on as well as
/// actually sending enough requests to see if there is a meaningful difference
/// between parallel + blocking and async
const KRATES: &[&str] = &[
    "ab_glyph",
    "ab_glyph_rasterizer",
    "accesskit",
    "addr2line",
    "adler",
    "aead",
    "aes",
    "aes-gcm",
    "ahash",
    "aho-corasick",
    "alsa",
    "alsa-sys",
    "ambient-authority",
    "android-activity",
    "android-properties",
    "anstream",
    "anstyle",
    "anstyle-parse",
    "anstyle-query",
    "anstyle-wincon",
    "anyhow",
    "anymap2",
    "app_dirs2",
    "arbitrary",
    "array-init",
    "arrayvec",
    "ash",
    "ash-molten",
    "ash-window",
    "assert-json-diff",
    "async-backtrace",
    "async-backtrace-attributes",
    "async-channel",
    "async-compression",
    "async-io",
    "async-lock",
    "async-recursion",
    "async-stream",
    "async-stream-impl",
    "async-trait",
    "atk-sys",
    "atomic_refcell",
    "autocfg",
    "autometrics",
    "autometrics-macros",
    "axum",
    "axum-core",
    "axum-extra",
    "axum-macros",
    "backtrace",
    "base-x",
    "base64",
    "bb8",
    "bb8-postgres",
    "bincode",
    "bindgen",
    "bit-set",
    "bit-vec",
    "bitflags",
    "bitvec",
    "block",
    "block-buffer",
    "block-sys",
    "block2",
    "bstr",
    "bumpalo",
    "bytemuck",
    "bytemuck_derive",
    "byteorder",
    "bytes",
    "bytes-varint",
    "cairo-sys-rs",
    "calloop",
    "camino",
    "cap-fs-ext",
    "cap-primitives",
    "cap-rand",
    "cap-std",
    "cap-time-ext",
    "cargo-manifest",
    "cargo-platform",
    "cargo_metadata",
    "cc",
    "cervo-asset",
    "cervo-core",
    "cervo-nnef",
    "cervo-onnx",
    "cervo-runtime",
    "cesu8",
    "cexpr",
    "cfg-expr",
    "cfg-if",
    "cfg_aliases",
    "cint",
    "cipher",
    "clang-sys",
    "clap",
    "clap_builder",
    "clap_derive",
    "clap_lex",
    "clipboard-win",
    "cocoa",
    "cocoa-foundation",
    "color_quant",
    "colorchoice",
    "combine",
    "concurrent-queue",
    "console",
    "console-api",
    "console-subscriber",
    "cookie",
    "copypasta",
    "core-foundation",
    "core-foundation-sys",
    "core-graphics",
    "core-graphics-types",
    "coreaudio-rs",
    "coreaudio-sys",
    "coremidi",
    "coremidi-sys",
    "cpp_demangle",
    "cpufeatures",
    "cranelift-bforest",
    "cranelift-codegen",
    "cranelift-codegen-meta",
    "cranelift-codegen-shared",
    "cranelift-control",
    "cranelift-entity",
    "cranelift-frontend",
    "cranelift-isle",
    "cranelift-native",
    "cranelift-wasm",
    "crash-context",
    "crash-handler",
    "crc32fast",
    "crossbeam-channel",
    "crossbeam-deque",
    "crossbeam-epoch",
    "crossbeam-utils",
    "crunchy",
    "crypto-common",
    "ctr",
    "custom_debug",
    "custom_debug_derive",
    "darling",
    "darling_core",
    "darling_macro",
    "dashmap",
    "dasp_sample",
    "data-encoding",
    "data-encoding-macro",
    "data-encoding-macro-internal",
    "data-url",
    "debugid",
    "derive-new",
    "derive_arbitrary",
    "derive_builder",
    "derive_builder_core",
    "derive_builder_macro",
    "derive_more",
    "digest",
    "dirs",
    "dirs-sys",
    "discord-sdk",
    "dispatch",
    "dmsort",
    "doc-comment",
    "dolly",
    "downcast-rs",
    "dyn-clone",
    "ecolor",
    "educe",
    "egui",
    "egui-winit",
    "either",
    "emath",
    "embed-resource",
    "encode_unicode",
    "endian-type",
    "enum-ordinalize",
    "enum-primitive-derive",
    "enumn",
    "env_logger",
    "epaint",
    "errno",
    "errno-dragonfly",
    "euclid",
    "event-listener",
    "fallible-iterator",
    "fastrand",
    "fd-lock",
    "fdeflate",
    "filetime",
    "findshlibs",
    "fixedbitset",
    "fixedvec",
    "flate2",
    "float-cmp",
    "float_eq",
    "float_next_after",
    "fnv",
    "foreign-types",
    "foreign-types-shared",
    "form_urlencoded",
    "fs-set-times",
    "fs2",
    "fsevent-sys",
    "funty",
    "futures",
    "futures-channel",
    "futures-core",
    "futures-executor",
    "futures-io",
    "futures-lite",
    "futures-macro",
    "futures-sink",
    "futures-task",
    "futures-util",
    "fxhash",
    "fxprof-processed-profile",
    "gdk-pixbuf-sys",
    "gdk-sys",
    "generator",
    "generic-array",
    "gethostname",
    "getrandom",
    "ghash",
    "gimli",
    "gio-sys",
    "glam",
    "glib-sys",
    "glob",
    "gltf",
    "gltf-derive",
    "gltf-json",
    "gobject-sys",
    "goblin",
    "google-cloud-gax",
    "google-cloud-googleapis",
    "google-cloud-pubsub",
    "google-cloud-token",
    "gpu-allocator",
    "gtk-sys",
    "h2",
    "half",
    "hashbag",
    "hashbrown",
    "hdrhistogram",
    "headers",
    "headers-core",
    "heck",
    "hermit-abi",
    "hex",
    "highway",
    "hmac",
    "home",
    "hound",
    "http",
    "http-body",
    "http-range-header",
    "httparse",
    "httpdate",
    "humantime",
    "hyper",
    "hyper-rustls",
    "hyper-timeout",
    "ident_case",
    "idna",
    "image",
    "include_dir",
    "include_dir_macros",
    "indexmap",
    "inflections",
    "inotify",
    "inotify-sys",
    "inout",
    "insta",
    "instant",
    "io-extras",
    "io-kit-sys",
    "io-lifetimes",
    "ipnet",
    "iri-string",
    "is-terminal",
    "itertools",
    "itoa",
    "ittapi",
    "ittapi-sys",
    "jni",
    "jni-sys",
    "jobserver",
    "js-sys",
    "jsonwebtoken",
    "kqueue",
    "kqueue-sys",
    "kstring",
    "lazy-bytes-cast",
    "lazy_static",
    "lazycell",
    "leb128",
    "lewton",
    "libc",
    "libloading",
    "libm",
    "libmimalloc-sys",
    "libudev-sys",
    "line-wrap",
    "linked-hash-map",
    "linux-raw-sys",
    "liquid",
    "liquid-core",
    "liquid-derive",
    "liquid-lib",
    "lock_api",
    "log",
    "loom",
    "lyon_geom",
    "lyon_path",
    "lyon_tessellation",
    "lz4_flex",
    "mach",
    "mach2",
    "malloc_buf",
    "maplit",
    "mapr",
    "matchers",
    "matchit",
    "matrixmultiply",
    "maybe-owned",
    "md-5",
    "memchr",
    "memfd",
    "memmap2",
    "memoffset",
    "metal",
    "metrics",
    "metrics-exporter-prometheus",
    "metrics-macros",
    "metrics-util",
    "mimalloc",
    "mime",
    "minidump-common",
    "minidump-writer",
    "minidumper",
    "minimal-lexical",
    "miniz_oxide",
    "mio",
    "mockito",
    "multibase",
    "multimap",
    "named_pipe",
    "natord",
    "ndarray",
    "ndk",
    "ndk-context",
    "ndk-sys",
    "nibble_vec",
    "nix",
    "no-std-compat",
    "nohash-hasher",
    "nom",
    "normpath",
    "notify",
    "ntapi",
    "nu-ansi-term",
    "num-bigint",
    "num-complex",
    "num-derive",
    "num-integer",
    "num-rational",
    "num-traits",
    "num_cpus",
    "num_enum",
    "num_enum_derive",
    "objc",
    "objc-foundation",
    "objc-sys",
    "objc2",
    "objc2-encode",
    "objc_exception",
    "objc_id",
    "object",
    "oboe",
    "oboe-sys",
    "ogg",
    "once_cell",
    "opaque-debug",
    "openapiv3",
    "opener",
    "openssl-probe",
    "opentelemetry",
    "opentelemetry-http",
    "opentelemetry-otlp",
    "opentelemetry-proto",
    "opentelemetry-semantic-conventions",
    "opentelemetry-zipkin",
    "opentelemetry_api",
    "opentelemetry_sdk",
    "orbclient",
    "ordered-float",
    "os_info",
    "overload",
    "owned_ttf_parser",
    "pango-sys",
    "paranoid-android",
    "parking",
    "parking_lot",
    "parking_lot_core",
    "paste",
    "path_abs",
    "peeking_take_while",
    "pem",
    "percent-encoding",
    "perchance",
    "pest",
    "pest_derive",
    "pest_generator",
    "pest_meta",
    "petgraph",
    "phf",
    "phf_shared",
    "physx",
    "physx-sys",
    "pin-project",
    "pin-project-internal",
    "pin-project-lite",
    "pin-utils",
    "pkg-config",
    "plain",
    "plist",
    "png",
    "polling",
    "polyval",
    "portable-atomic",
    "postgres-protocol",
    "postgres-types",
    "pprof",
    "ppv-lite86",
    "presser",
    "prettyplease",
    "proc-macro-crate",
    "proc-macro-error",
    "proc-macro-error-attr",
    "proc-macro2",
    "prost",
    "prost-build",
    "prost-derive",
    "prost-types",
    "psm",
    "public-api",
    "puffin",
    "puffin_egui",
    "puffin_http",
    "quanta",
    "quick-xml",
    "quickcheck",
    "quickcheck_macros",
    "quote",
    "radium",
    "radix_trie",
    "rand",
    "rand_chacha",
    "rand_core",
    "rand_distr",
    "range-map",
    "raw-cpuid",
    "raw-window-handle",
    "raw-window-metal",
    "rawpointer",
    "rayon",
    "rayon-core",
    "redis-async",
    "redox_syscall",
    "redox_users",
    "regalloc2",
    "regex",
    "regex-automata",
    "regex-syntax",
    "ring",
    "ron",
    "rspirv",
    "rspirv-reflect",
    "rustc-demangle",
    "rustc-hash",
    "rustc_version",
    "rustdoc-json",
    "rustdoc-types",
    "rustix",
    "rustls",
    "rustls-native-certs",
    "rustls-pemfile",
    "rustls-webpki",
    "rustversion",
    "rymder",
    "ryu",
    "sadness-generator",
    "safemem",
    "same-file",
    "scan_fmt",
    "schannel",
    "schemars",
    "schemars_derive",
    "scoped-tls",
    "scopeguard",
    "scroll",
    "scroll_derive",
    "sct",
    "security-framework",
    "security-framework-sys",
    "self_cell",
    "semver",
    "sentry-types",
    "serde",
    "serde_derive",
    "serde_derive_internals",
    "serde_json",
    "serde_path_to_error",
    "serde_qs",
    "serde_repr",
    "serde_spanned",
    "serde_urlencoded",
    "serde_yaml",
    "serial_test",
    "serial_test_derive",
    "sha1",
    "sha2",
    "sharded-slab",
    "shellexpand",
    "shlex",
    "signal-hook",
    "signal-hook-registry",
    "simd-adler32",
    "similar",
    "simple_asn1",
    "siphasher",
    "sketches-ddsketch",
    "slab",
    "sled",
    "slice-group-by",
    "slotmap",
    "smallvec",
    "smart-default",
    "socket2",
    "spez",
    "spin",
    "spirv",
    "spirv-std",
    "spirv-std-macros",
    "spirv-std-types",
    "sptr",
    "stable-vec",
    "stable_deref_trait",
    "static_assertions",
    "std_prelude",
    "stfu8",
    "string-interner",
    "stringprep",
    "strsim",
    "strum",
    "strum_macros",
    "subtle",
    "superluminal-perf",
    "superluminal-perf-sys",
    "symbolic-common",
    "symbolic-debuginfo",
    "symbolic-demangle",
    "syn",
    "sync_wrapper",
    "synstructure",
    "sysinfo",
    "system-deps",
    "system-interface",
    "tame-gcs",
    "tame-oauth",
    "tame-oidc",
    "tame-webpurify",
    "tap",
    "tar",
    "target-lexicon",
    "tempfile",
    "thiserror",
    "thiserror-impl",
    "thread_local",
    "time",
    "time-core",
    "time-macros",
    "tiny-bench",
    "tinyvec",
    "tinyvec_macros",
    "tokio",
    "tokio-io-timeout",
    "tokio-macros",
    "tokio-postgres",
    "tokio-retry",
    "tokio-rustls",
    "tokio-stream",
    "tokio-test",
    "tokio-tungstenite",
    "tokio-util",
    "toml",
    "toml_datetime",
    "toml_edit",
    "tonic",
    "tower",
    "tower-http",
    "tower-layer",
    "tower-service",
    "tracing",
    "tracing-appender",
    "tracing-attributes",
    "tracing-core",
    "tracing-futures",
    "tracing-log",
    "tracing-logfmt",
    "tracing-opentelemetry",
    "tracing-subscriber",
    "tract-core",
    "tract-data",
    "tract-hir",
    "tract-nnef",
    "tract-onnx",
    "tract-onnx-opl",
    "tracy-client",
    "tracy-client-sys",
    "try-lock",
    "tryhard",
    "ttf-parser",
    "tungstenite",
    "twox-hash",
    "typed-builder",
    "typenum",
    "ucd-trie",
    "uds",
    "uname",
    "unicode-bidi",
    "unicode-ident",
    "unicode-normalization",
    "unicode-segmentation",
    "unicode-xid",
    "universal-hash",
    "unsafe-libyaml",
    "untrusted",
    "url",
    "urlencoding",
    "utf-8",
    "utf8parse",
    "uuid",
    "valuable",
    "vec1",
    "vec_map",
    "version-compare",
    "version_check",
    "vswhom",
    "vswhom-sys",
    "waker-fn",
    "walkdir",
    "want",
    "wasi",
    "wasi-cap-std-sync",
    "wasi-common",
    "wasm-bindgen",
    "wasm-bindgen-backend",
    "wasm-bindgen-futures",
    "wasm-bindgen-macro",
    "wasm-bindgen-macro-support",
    "wasm-bindgen-shared",
    "wasmbin",
    "wasmbin-derive",
    "wasmparser",
    "wasmtime",
    "wasmtime-asm-macros",
    "wasmtime-cranelift",
    "wasmtime-cranelift-shared",
    "wasmtime-environ",
    "wasmtime-jit",
    "wasmtime-jit-debug",
    "wasmtime-jit-icache-coherence",
    "wasmtime-runtime",
    "wasmtime-types",
    "wasmtime-wasi",
    "wast",
    "web-sys",
    "webbrowser",
    "webpki-roots",
    "which",
    "wiggle",
    "wiggle-generate",
    "wiggle-macro",
    "winapi",
    "winapi-i686-pc-windows-gnu",
    "winapi-util",
    "winapi-wsapoll",
    "winapi-x86_64-pc-windows-gnu",
    "windows",
    "windows-core",
    "windows-sys",
    "windows-targets",
    "windows_aarch64_gnullvm",
    "windows_aarch64_msvc",
    "windows_i686_gnu",
    "windows_i686_msvc",
    "windows_x86_64_gnu",
    "windows_x86_64_gnullvm",
    "windows_x86_64_msvc",
    "winnow",
    "winreg",
    "winx",
    "witx",
    "wyz",
    "x11-clipboard",
    "x11-dl",
    "x11rb",
    "x11rb-protocol",
    "xattr",
    "xdg",
    "yaml-rust",
    "zip",
    "zstd",
    "zstd-safe",
    "zstd-sys",
];
