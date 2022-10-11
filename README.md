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

By default, the `buildid` crate will pick the best build-id lookup function it
can for your platform. If one is not avaliable, it may fail to compile. If you
have a custom build-id lookup mechanism you want to tell `buildid` about,
enabling one of the features may help.

## License

Licensed under Mozilla Public License 2.0

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you shall be licensed as above, without any
additional terms or conditions.
