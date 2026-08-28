# buildid: get the buildid from your (running) library or executable

Get a `&'static [u8]` that is unique to the current binary.

```rust
println!("{:?}", buildid::build_id())
```

A build-id is a value which is guaranteed to change when any of the component
objects of a binary change. A change in the build-id does not guarantee that
the executable or it's components are actually different. Two distinct
executables may have a different build-id if they were modified after linking
(for example, by `chrpath` or similar).

A build-id is intended to be sufficient to identify the appropriate debug
information to use for a given object, and is used for this purpose by `gdb`
and other debuggers.

Both executables and shared objects contain build-ids. Using
`buildid::build_id()` will return the build-id for the object that includes
`buildid` (this crate). For example, if you write a shared object (shared
library) using this crate, and provide a function which returns the build-id
(by calling `buildid::build_id()` internally) in the shared objects's external
API, that function call will return the build-id of the shared object/library
(not the executable).

Windows, MacOS, and Linux are supported (and work automatically). Embedded and
other platforms can be supported by enabling one of the optional features (see
the [docs](https://docs.rs/buildid) for details). If you have another platform
that needs support, send a PR!

## WebAssembly and WASI

WebAssembly modules cannot inspect their own custom sections at runtime. To
make the build ID available both to the guest and to host tooling, run the
companion stamper on the final `.wasm` file:

```console
RUSTFLAGS="-C link-arg=--build-id" \
  cargo build --target wasm32-unknown-unknown --release
cargo run -p buildid-wasm-stamp -- \
  target/wasm32-unknown-unknown/release/application.wasm
```

When using the published crates, install and run the companion directly:

```console
cargo install buildid-wasm-stamp
buildid-wasm-stamp application.wasm
buildid-wasm-stamp --check application.wasm
```

By default, `--id-source top-level` copies the input binary's top-level
`build_id` into every guest-visible slot. For a core module this is
its own ID; for a component it is the outer component's ID. To generate a
canonical SHA-256 ID instead, select content mode explicitly:

```console
buildid-wasm-stamp --id-source content application.wasm
```

In either mode, the guest slot and conventional WebAssembly `build_id` custom
section contain the same bytes. `buildid::build_id()` returns that exact stamp.
It returns `None` when the linked module has not been stamped.

Stamping is deterministic and idempotent. Run it after `wasm-opt`,
`wasm-bindgen`, componentization, debug stripping, and any other tool that can
rewrite the WebAssembly binary. By default the input is replaced; use
`--output PATH` to preserve it.

In both modes every stamp slot receives the same outermost-binary ID. This
supports multiple copies of the `buildid` library and multiple embedded core
modules.

By default, the `buildid` crate will pick the best build-id lookup function it
can for your platform. If one is not available, it may fail to compile. If you
have a custom build-id lookup mechanism you want to tell `buildid` about,
enabling one of the features may help.

## License

Licensed under Mozilla Public License 2.0

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you shall be licensed as above, without any
additional terms or conditions.
