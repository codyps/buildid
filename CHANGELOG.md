# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.1.0](https://github.com/codyps/buildid/compare/buildid-v1.0.5...buildid-v1.1.0) - 2026-10-28

### Added

- Add guest-readable WebAssembly build IDs through `buildid::build_id()` after
  post-link stamping with the `buildid-wasm-stamp` companion tool. Unstamped
  modules continue to return `None`.

### Fixed

- Refresh Nix inputs and request incremental MSVC links in CI.
- Replay stable inputs for Windows incremental linking.

## [1.0.5](https://github.com/codyps/buildid/compare/buildid-v1.0.4...buildid-v1.0.5) - 2026-08-17

### Fixed

- Fix macOS feature-powerset build
- Fix Windows incremental build IDs ([#17](https://github.com/codyps/buildid/pull/17))
