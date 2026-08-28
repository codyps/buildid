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
the hexadecimal ID on standard output.

By default, `--id-source top-level` copies the input binary's top-level
`build_id` into every guest-visible slot. For a core module, this is
the module's own ID. For a component, every slot in every embedded core module
receives the outer component's ID. The ID must contain between 1 and 32 bytes,
and the slot records top-level provenance.

If the program has no existing build ID, generate a deterministic SHA-256 ID:

```console
buildid-wasm-stamp --id-source content application.wasm
```

The generated ID is SHA-256 over a canonical form of the file in which
top-level `build_id` custom sections are absent and the guest slot contains its
versioned unstamped value. Generating the ID for the same file again produces
byte-for-byte identical output.
