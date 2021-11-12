# buildid: get the buildid from your (running) library or executable

```rust
println!("{:?}", buildid::build_id())
```

build-id is a value which is guaranteed to change when any of the component objects of a binary
change. A change in the build-id does not guarantee that the executable or it's components are
actually different. Two distinct executables may have a different build-id if they were
modified after linking (for example, by `chrpath` or similar).

build-id is intended to be sufficient to identify the appropriate debug information to use for
a given object, and is used for this purpose by `gdb` and other debuggers.

build-id is also used by `mesa` as a key component in their caching of shaders (changes in the
build-id cause the cache to be discarded).

Executables and shared objects contain build-ids. Using `buildid::build_id()` will return the
build-id for the object that includes `buildid` (this crate). For example, if you write a
shared object (shared library) using this crate, and provide a way to get the build-id in it's
external API, that call will return the build-id of the shared object/library (not the
executable).

By default, the `buildid` crate will pick the best build-id lookup function it can for your
platform. If one is not avaliable, it may fail to compile. If you have a custom build-id lookup
mechanism you want to tell `buildid` about, enabling one of the features may help.

## License

Licensed under either of
 * [CDDL-1.0](https://opensource.org/licenses/cddl1.txt) or
 * [GPL-2 or later](https://www.gnu.org/licenses/old-licenses/gpl-2.0.en.html)

At your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you shall be dual licensed as above, without any
additional terms or conditions. You further grant the right to the Initial
Developer to license the submitted contribution under additional licenses that
serve similar purposes to the ones currently listed above.
