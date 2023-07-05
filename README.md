<!-- Allow this file to not have a first line heading -->
<!-- markdownlint-disable-file MD041 no-emphasis-as-heading -->

<!-- inline html -->
<!-- markdownlint-disable-file MD033 -->

<div align="center">

# `📇 tame-index`

**Small crate for interacting with [cargo registry indices](https://doc.rust-lang.org/nightly/cargo/reference/registry-index.html)**

[![Embark](https://img.shields.io/badge/embark-open%20source-blueviolet.svg)](https://embark.dev)
[![Embark](https://img.shields.io/badge/discord-ark-%237289da.svg?logo=discord)](https://discord.gg/dAuKfZS)
[![Crates.io](https://img.shields.io/crates/v/tame-index.svg)](https://crates.io/crates/tame-index)
[![Docs](https://docs.rs/tame-index/badge.svg)](https://docs.rs/tame-index)
[![dependency status](https://deps.rs/repo/github/EmbarkStudios/tame-index/status.svg)](https://deps.rs/repo/github/EmbarkStudios/tame-index)
[![Build status](https://github.com/EmbarkStudios/tame-index/workflows/CI/badge.svg)](https://github.com/EmbarkStudios/tame-index/actions)
</div>

## You probably want to use [`crates-index`][0]

This crate is a hard fork of [`crates-index`][0] to fit the use cases of [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny) and [`cargo-fetcher`](https://github.com/EmbarkStudios/cargo-fetcher), if you are looking for a crate to access a cargo registry index, you would be well served by using [`crates-index`][0] instead.

## Differences from [`crates-index`][0]

If you still want to use this crate instead, there are some differences to be aware of. Though note any of these may change in the future as [`crates-index`][0] and this crate evolve.

1. Git registry support is optional, gated behind the `git` feature flag.
1. Git functionality is provided by [`gix`](https://crates.io/crates/gix) instead of [`git2`](https://crates.io/crates/git2)
1. The API exposes enough pieces where an alternative git implementation can be used if `gix` is not to your liking.
1. Sparse index support is optional, gated behind the `sparse` feature flag.
1. Sparse HTTP functionality is provided by the [`reqwest`](https://crates.io/crates/reqwest) crate
1. Support for creating HTTP requests and parsing HTTP responses is still available when the `sparse` feature is not enabled if you want to use a different HTTP client than `reqwest`
1. Local cache files are always supported regardless of features enabled
1. Functionality for determining the local index location for a remote registry URL is exposed in the public API
1. Functionality for writing cache entries to the local index cache is exposed in the public API

## Contributing

[![Contributor Covenant](https://img.shields.io/badge/contributor%20covenant-v1.4-ff69b4.svg)](CODE_OF_CONDUCT.md)

We welcome community contributions to this project.

Please read our [Contributor Guide](CONTRIBUTING.md) for more information on how to get started.
Please also read our [Contributor Terms](CONTRIBUTING.md#contributor-terms) before you make any contributions.

Any contribution intentionally submitted for inclusion in an Embark Studios project, shall comply with the Rust standard licensing model (MIT OR Apache 2.0) and therefore be dual licensed as described below, without any additional terms or conditions:

### License

This contribution is dual licensed under EITHER OF

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

For clarity, "your" refers to Embark or any other licensee/user of the contribution.

[0]: https://crates.io/crates/crates-index
