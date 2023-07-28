<!-- markdownlint-disable blanks-around-headings blanks-around-lists no-duplicate-heading -->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->
## [Unreleased] - ReleaseDate
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
[Unreleased]: https://github.com/EmbarkStudios/tame-index/compare/0.2.4...HEAD
[0.2.4]: https://github.com/EmbarkStudios/tame-index/compare/0.2.3...0.2.4
[0.2.3]: https://github.com/EmbarkStudios/tame-index/compare/0.2.2...0.2.3
[0.2.2]: https://github.com/EmbarkStudios/tame-index/compare/0.2.1...0.2.2
[0.2.1]: https://github.com/EmbarkStudios/tame-index/compare/0.2.0...0.2.1
[0.2.0]: https://github.com/EmbarkStudios/tame-index/compare/0.1.0...0.2.0
[0.1.0]: https://github.com/EmbarkStudios/tame-index/compare/0.0.1...0.1.0
[0.0.1]: https://github.com/EmbarkStudios/tame-index/releases/tag/0.0.1
