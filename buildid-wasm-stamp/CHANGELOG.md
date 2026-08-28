# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Add the `buildid-wasm-stamp` companion tool for WebAssembly modules and
  components.
- Copy the top-level module or component build ID into every guest-visible slot
  by default with `--id-source top-level`.
- Generate a deterministic content-derived ID with `--id-source content`.
- Stamp and validate every slot, supporting multiple copies of the `buildid`
  library and multiple core modules within a component.
- Suppress build-ID output with `--quiet`.
