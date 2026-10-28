# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/codyps/buildid/compare/buildid-wasm-stamp-v0.0.0...buildid-wasm-stamp-v0.1.0) - 2026-10-28

### Added

- Add the `buildid-wasm-stamp` companion tool for WebAssembly modules and
  components.
- Copy each containing core module's existing build ID into its guest-visible
  slots by default with `--id-source module`, including inside components.
- Copy an outer module or component ID into every slot explicitly with
  `--id-source top-level`.
- Generate a deterministic content-derived ID with `--id-source content`.
- Stamp and validate every slot, supporting multiple copies of the `buildid`
  library and multiple core modules within a component.
- Suppress build-ID output with `--quiet`.
- Locate slots only in parsed data-segment payloads, preserving matching bytes
  in custom sections and other parts of the binary.

### Changed

- Stream content hashing and output construction without full-file staging
  copies, and reuse the parsed layout when checking content-derived IDs.
- Borrow existing build-ID payloads and iterate slots without cloning their
  metadata.
