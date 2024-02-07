<!-- markdownlint-disable blanks-around-headings blanks-around-lists no-duplicate-heading -->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->
## [Unreleased] - ReleaseDate
### Fixed
- [PR#47](https://github.com/EmbarkStudios/tame-index/pull/47) fixed [#46](https://github.com/EmbarkStudios/tame-index/issues/46) by ensuring one full DNS lookup and request response roundtrip is made before going wide to ensure that excessive DNS lookups and connections are not made.

## [0.9.2] - 2024-01-21
### Changed
- [PR#45](https://github.com/EmbarkStudios/tame-index/pull/45) bumped `gix` -> 0.58

## [0.9.1] - 2024-01-12
### Changed
- [PR#44](https://github.com/EmbarkStudios/tame-index/pull/44) bumped `gix` -> 0.57

## [0.9.0] - 2023-12-13
### Fixed
- [PR#43](https://github.com/EmbarkStudios/tame-index/pull/43) fixed the file lock options from `LockOptions::cargo_package_lock` to be `exclusive` to more closely match Cargo's behavior. This would not have been a problem in practice, but is more correct now.

### Changed
- [PR#43](https://github.com/EmbarkStudios/tame-index/pull/43) bumped `gix` -> 0.56

## [0.8.0] - 2023-11-06
### Fixed
- [PR#41](https://github.com/EmbarkStudios/tame-index/pull/41) resolved [#29](https://github.com/EmbarkStudios/tame-index/issues/29) by force disabling gpg signing in test.
- Commit e3c6ff1 bumped the patch version of `windows-targets` to .5 to prevent using older versions that don't compile (See [#40](https://github.com/EmbarkStudios/tame-index/issues/40))

### Changed
- [PR#41](https://github.com/EmbarkStudios/tame-index/pull/41) bumped `gix` -> 0.55

## [0.7.2] - 2023-10-18
### Fixed
- [PR#39](https://github.com/EmbarkStudios/tame-index/pull/39) resolved [#38](https://github.com/EmbarkStudios/tame-index/issues/38) by ensuring all parent directories are created before attempting a clone with `gix`.

## [0.7.1] - 2023-09-29
### Fixed
- [PR#34](https://github.com/EmbarkStudios/tame-index/pull/33) resolved a compile issue when targeting `musl` libc.

## [0.7.0] - 2023-09-29
### Changed
- [PR#32](https://github.com/EmbarkStudios/tame-index/pull/32) resolved [#31](https://github.com/EmbarkStudios/tame-index/issues/31) by reducing the size of `Error`.
- [PR#33](https://github.com/EmbarkStudios/tame-index/pull/33) updated dependencies, notably `gix` -> 0.54.
- [PR#33](https://github.com/EmbarkStudios/tame-index/pull/33) added a `tame_index::utils::flock::FileLock` parameter to all methods on indices that perform disk operations.

### Added
- [PR#33](https://github.com/EmbarkStudios/tame-index/pull/33) added `tame_index::utils::flock`, which contains a `FileLock` for holding an OS file lock for a particular path, as well as `LockOptions` for creating them.

### Fixed
- [PR#33](https://github.com/EmbarkStudios/tame-index/pull/33) resolved [#30](https://github.com/EmbarkStudios/tame-index/issues/30) by removing the usage of `gix::lock` in favor of the aforementioned `FileLock`
- [PR#33](https://github.com/EmbarkStudios/tame-index/pull/33) resolved [#17](https://github.com/EmbarkStudios/tame-index/issues/17) by adding `LockOptions::cargo_package_lock` to easily create a lock file compatible with cargo's own ($CARGO_HOME global) package lock.

## [0.6.0] - 2023-09-11
### Changed
- [PR#27](https://github.com/EmbarkStudios/tame-index/pull/27) updated `gix` to 0.53.1. Thanks [@Byron](https://github.com/Byron)!

## [0.5.6] - 2023-09-11 **yanked**
### Changed
- [PR#27](https://github.com/EmbarkStudios/tame-index/pull/27) updated `gix` to 0.53.1. Thanks [@Byron](https://github.com/Byron)!

## [0.5.5] - 2023-09-06
### Changed
- [PR#26](https://github.com/EmbarkStudios/tame-index/pull/26) changed sparse index request creation to not use HTTP/2 for the version to support corporate potato proxies. This results in a slight but noticeable degradation in throughput when making many requests to a sparse index.

## [0.5.4] - 2023-08-24
### Fixed
- [PR#24](https://github.com/EmbarkStudios/tame-index/pull/24) resolved [#23](https://github.com/EmbarkStudios/tame-index/issues/23) by fixing a bug where index cache paths were not lower cased as cargo does.

## [0.5.3] - 2023-08-23
### Fixed
- [PR#22](https://github.com/EmbarkStudios/tame-index/pull/22) fixed an issue where ssh index urls would be mapped to the incorrect local directory. This issue was raised in [cargo-deny](https://github.com/EmbarkStudios/cargo-deny/issues/548).

## [0.5.2] - 2023-08-23
### Fixed
- [`d9cb55f`] fixed and issue with docs.rs documentation building.

## [0.5.1] - 2023-08-23
### Added
- [PR#20](https://github.com/EmbarkStudios/tame-index/pull/20) publicly exposed `tame_index::external::http` for easier downstream usage.

## [0.5.0] - 2023-08-23
### Fixed
- [PR#18](https://github.com/EmbarkStudios/tame-index/pull/18) resolved [#16](https://github.com/EmbarkStudios/tame-index/issues/16) by marking `ComboIndexCache` and `ComboIndex` as `#[non_exhaustive]`. This avoids build breaks if the `local` feature is enabled in one transitive dependency and not in another, as much as I hate `non_exhaustive`.

### Changed
- [PR#18](https://github.com/EmbarkStudios/tame-index/pull/18) changed `SparseIndex::make_remote_request` to take an optional [ETag](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/ETag), completely avoiding disk I/O, which allows `SparseIndex` to be used for making and parsing requests without worrying about cargo's global package lock.

## [0.4.1] - 2023-08-21
### Added
- [PR#15](https://github.com/EmbarkStudios/tame-index/pull/15) added the `native-certs` feature to be able to use the OS certificate store instead of `webpki-roots`. Thanks [@Shnatsel](https://github.com/Shnatsel)!

## [0.4.0] - 2023-08-18
### Changed
- [PR#14](https://github.com/EmbarkStudios/tame-index/pull/14) added the ability to specify the repository lock policy when calling `RemoteGitIndex::with_options`. Thanks [@Shnatsel](https://github.com/Shnatsel)!

## [0.3.2] - 2023-08-15
### Fixed
- [PR#13](https://github.com/EmbarkStudios/tame-index/pull/13) fixed a bug where git repository url canonicalization was incorrect if the url was not a github.com url that ended with .git.

## [0.3.1] - 2023-08-04
### Added
- [PR#11](https://github.com/EmbarkStudios/tame-index/pull/11) added `RemoteSparseIndex::krates`, `AsyncRemoteSparseIndex::krates`, and `AsyncRemoteSparseIndex::krates_blocking` as helper methods for improving throughput when fetching index entries for many crates.

## [0.3.0] - 2023-08-03
### Changed
- [PR#10](https://github.com/EmbarkStudios/tame-index/pull/10) unfortunately had to [relax the constraint](https://github.com/rustsec/rustsec/issues/759) that crate versions in an index are always parsable as `semver::Version`.

## [0.2.5] - 2023-08-02
### Fixed
- [PR#9](https://github.com/EmbarkStudios/tame-index/pull/9) resolved [#8](https://github.com/EmbarkStudios/tame-index/issues/8) by ensuring (valid) non-official cargo build version output can also be parsed.

## [0.2.4] - 2023-07-28
### Fixed
- [PR#7](https://github.com/EmbarkStudios/tame-index/pull/7) fixed an issue where `RemoteGitIndex::fetch` could fail in environments where the git committer was not configured.

### Changed
- [PR#7](https://github.com/EmbarkStudios/tame-index/pull/7) change how `RemoteGitIndex` looks up blobs. Previously fetching would actually update references, now however we write a `FETCH_HEAD` similarly to git/libgit2, and uses that (or other) reference to find the commit to use, rather than updating the HEAD to point to the same commit as the remote HEAD.

## [0.2.3] - 2023-07-26
### Fixed
- [PR#6](https://github.com/EmbarkStudios/tame-index/pull/6) fixed two bugs with git registries.
    1. `cargo` does not set remotes for git registry indices, the previous code assumed there was a remote, thus failed to fetch updates
    2. Updating reflogs after a fetch would fail in CI-like environments without a global git config that set the committer, `committer.name` is now set to `tame-index`

## [0.2.2] - 2023-07-26
### Changed
- [PR#5](https://github.com/EmbarkStudios/tame-index/pull/5) relaxed `rust-version` to 1.67.0.

## [0.2.1] - 2023-07-26
### Added
- [PR#4](https://github.com/EmbarkStudios/tame-index/pull/4) added `GitError::is_spurious` and `GitError::is_locked` to detect fetch errors that could potentially succeed in the future if retried.

### Changed
- [PR#4](https://github.com/EmbarkStudios/tame-index/pull/4) now re-exports `reqwest` and `gix` from `tame_index::externals` for easier downstream usage.

## [0.2.0] - 2023-07-25
### Added
- [PR#3](https://github.com/EmbarkStudios/tame-index/pull/3) added support for [`Local Registry`](https://doc.rust-lang.org/cargo/reference/source-replacement.html#local-registry-sources)
- [PR#3](https://github.com/EmbarkStudios/tame-index/pull/3) added [`LocalRegistry`] as an option for `ComboIndexCache`
- [PR#3](https://github.com/EmbarkStudios/tame-index/pull/3) added `KrateName::cargo` and `KrateName::crates_io` options for validating crates names against the (current) constraints of cargo and crates.io respectively.

### Changed
- [PR#3](https://github.com/EmbarkStudios/tame-index/pull/3) refactored how index initialization is performed by splitting out the individual pieces into a cleaner API, adding the types `IndexUrl`, `IndexPath`, and `IndexLocation`

### Fixed
- [PR#3](https://github.com/EmbarkStudios/tame-index/pull/3) fixed an issue where the .cache entries for a git index were not using the same cache version of cargo, as of 1.65.0+. cargo in those versions now uses the object id of the blob the crate is read from, rather than the `HEAD` commit hash, for more granular change detection.

## [0.1.0] - 2023-07-05
### Added
- [PR#1](https://github.com/EmbarkStudios/tame-index/pull/1) added the initial working implementation for this crate

## [0.0.1] - 2023-06-19
### Added
- Initial crate squat

<!-- next-url -->
[Unreleased]: https://github.com/EmbarkStudios/tame-index/compare/0.9.2...HEAD
[0.9.2]: https://github.com/EmbarkStudios/tame-index/compare/0.9.1...0.9.2
[0.9.1]: https://github.com/EmbarkStudios/tame-index/compare/0.9.0...0.9.1
[0.9.0]: https://github.com/EmbarkStudios/tame-index/compare/0.8.0...0.9.0
[0.8.0]: https://github.com/EmbarkStudios/tame-index/compare/0.7.2...0.8.0
[0.7.2]: https://github.com/EmbarkStudios/tame-index/compare/0.7.1...0.7.2
[0.7.1]: https://github.com/EmbarkStudios/tame-index/compare/0.7.0...0.7.1
[0.7.0]: https://github.com/EmbarkStudios/tame-index/compare/0.6.0...0.7.0
[0.6.0]: https://github.com/EmbarkStudios/tame-index/compare/0.5.6...0.6.0
[0.5.6]: https://github.com/EmbarkStudios/tame-index/compare/0.5.5...0.5.6
[0.5.5]: https://github.com/EmbarkStudios/tame-index/compare/0.5.4...0.5.5
[0.5.4]: https://github.com/EmbarkStudios/tame-index/compare/0.5.3...0.5.4
[0.5.3]: https://github.com/EmbarkStudios/tame-index/compare/0.5.2...0.5.3
[0.5.2]: https://github.com/EmbarkStudios/tame-index/compare/0.5.1...0.5.2
[0.5.1]: https://github.com/EmbarkStudios/tame-index/compare/0.5.0...0.5.1
[0.5.0]: https://github.com/EmbarkStudios/tame-index/compare/0.4.1...0.5.0
[0.4.1]: https://github.com/EmbarkStudios/tame-index/compare/0.4.0...0.4.1
[0.4.0]: https://github.com/EmbarkStudios/tame-index/compare/0.3.2...0.4.0
[0.3.2]: https://github.com/EmbarkStudios/tame-index/compare/0.3.1...0.3.2
[0.3.1]: https://github.com/EmbarkStudios/tame-index/compare/0.3.0...0.3.1
[0.3.0]: https://github.com/EmbarkStudios/tame-index/compare/0.2.5...0.3.0
[0.2.5]: https://github.com/EmbarkStudios/tame-index/compare/0.2.4...0.2.5
[0.2.4]: https://github.com/EmbarkStudios/tame-index/compare/0.2.3...0.2.4
[0.2.3]: https://github.com/EmbarkStudios/tame-index/compare/0.2.2...0.2.3
[0.2.2]: https://github.com/EmbarkStudios/tame-index/compare/0.2.1...0.2.2
[0.2.1]: https://github.com/EmbarkStudios/tame-index/compare/0.2.0...0.2.1
[0.2.0]: https://github.com/EmbarkStudios/tame-index/compare/0.1.0...0.2.0
[0.1.0]: https://github.com/EmbarkStudios/tame-index/compare/0.0.1...0.1.0
[0.0.1]: https://github.com/EmbarkStudios/tame-index/releases/tag/0.0.1
