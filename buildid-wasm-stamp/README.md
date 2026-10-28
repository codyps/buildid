# buildid-wasm-stamp

`buildid-wasm-stamp` copies a WebAssembly program's build ID into a guest-visible
slot provided by the [`buildid`](https://crates.io/crates/buildid) crate. The
same value is available:

- inside the guest through `buildid::build_id()`; and
- outside the guest through the conventional `build_id` custom section.

Run the stamper after every tool that can rewrite the Wasm binary:

```console
RUSTFLAGS="-C link-arg=--build-id" \
  cargo build --target wasm32-wasip1 --release
buildid-wasm-stamp target/wasm32-wasip1/release/application.wasm
buildid-wasm-stamp --check target/wasm32-wasip1/release/application.wasm
```

The input is replaced by default. Pass `--output PATH` to write a separate
file. Replacement is atomic. Pass `--quiet` when a build script does not need
the hexadecimal IDs on standard output. Both stamping and `--check` print one
ID per core module containing slots, in file order; IDs may repeat.

By default, `--id-source module` copies each containing core module's existing
`build_id` into its own slots. Components are traversed recursively, so native
`wasm32-wasip2` output and componentized WASI modules keep their original module
IDs. No outer component ID is required. Modules without slots are left alone,
and all existing custom sections are preserved byte for byte. Each ID must
contain between 1 and 32 bytes. The guest slot records module provenance so
`--check` compares it to the correct module's section.

To use the outermost binary's ID for every guest slot, explicitly select:

```console
buildid-wasm-stamp --id-source top-level application.wasm
```

This requires one existing top-level `build_id` section. In a component it
overrides every guest lookup with the outer component's ID, while leaving
nested module ID sections unchanged. Prefer module mode when matching guest
IDs to existing module debug information.

If a module containing slots has no build ID, module mode fails rather than
choosing a different identity. Generate an ID for the whole final binary with:

```console
buildid-wasm-stamp --id-source content application.wasm
```

The generated ID is SHA-256 over a canonical form of the file in which
top-level `build_id` custom sections are absent and every guest slot contains its
versioned unstamped value. Generating the ID for the same file again produces
byte-for-byte identical output.

Slot discovery parses nested modules and searches only individual data-segment
payloads, including active and passive segments. Matching markers in custom
sections, instructions, or across segment boundaries are not modified.

The parser borrows section payloads from the input. Content mode streams the
canonical bytes into SHA-256 and directly into the returned binary, avoiding
full-file canonicalization buffers. `--check` reuses the same parsed layout
for freshness checks without copying or reparsing the binary. The CLI still
reads the input file into memory; stamping additionally allocates the output.

## Testing

`cargo test -p buildid-wasm-stamp --all-targets --locked` exercises the parser,
slot boundaries, and ID scopes. `./scripts/test-wasm-components` also executes
real components in Wasmtime, including distinct modules in nested components,
both outer-ID modes, and release builds with fat LTO. It requires the
`wasm32-unknown-unknown`, `wasm32-wasip1`, and `wasm32-wasip2` Rust targets,
`wasm-tools`, and `wasmtime`. Set `WASI_ADAPTER` to the official
`wasi_snapshot_preview1.command.wasm` adapter. CI installs pinned tools and
verifies their release checksums before running these tests.
