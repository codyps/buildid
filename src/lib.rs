//! Examine the build-id and similar values for your library or executable
//!
//! ```
//! println!("{:?}", buildid::build_id())
//! ```
//!
//! build-id is a value which is guaranteed to change when any of the component objects of a binary
//! change. A change in the build-id does not guarantee that the executable or it's components are
//! actually different. Two distinct executables may have a different build-id if they were
//! modified after linking (for example, by `chrpath` or similar).
//!
//! build-id is intended to be sufficient to identify the appropriate debug information to use for
//! a given object, and is used for this purpose by `gdb` and other debuggers.
//!
//! build-id is also used by `mesa` as a key component in their caching of shaders (changes in the
//! build-id cause the cache to be discarded).
//!
//! Executables and shared objects contain build-ids. Using `buildid::build_id()` will return the
//! build-id for the object that includes `buildid` (this crate). For example, if you write a
//! shared object (shared library) using this crate, and provide a way to get the build-id in it's
//! external API, that call will return the build-id of the shared object/library (not the
//! executable).
//!
//! By default, the `buildid` crate will pick the best build-id lookup function it can for your
//! platform. If one is not available, it may fail to compile. If you have a custom build-id lookup
//! mechanism you want to tell `buildid` about, enabling one of the features may help.
//!
//! # Optional Features
//!
//! For all of the build-id lookup customization features, we recommend only setting them in
//! top-level crates than have a complete understanding of the final link step for the executable.
//!
//! ## `buildid-symbol-start-end`
//!
//! When enabled, assume the presence of 2 symbols named "__build_id_start" and "__build_id_end", and
//! use these to find the build-id. For gnu linkers, these symbols exist and can be used
//! automatically.
//!
//! If they aren't present for some reason, one can provide the symbols by using a custom
//! ldscript (linker script). See `buildid-linker-symbols` for a linker script mechanism to provide
//! these.
//!
//! This method takes precedence over automatically enable build-id
//! lookup methods, and over `buildid-section-inject`.
//!
//! ## `buildid-custom-inject`
//!
//! When enabled, assume that a function `int build_id__get(unsigned char **build_id, size_t *len)`
//! is provided (with C linkage) that can locate and return the build-id. The `build_id__get` must
//! return `1` if a build-id is located (and modify the `build_id` and `len` arguments to point to
//! the memory containing the build-id and to contain the number of bytes in the build-id
//! respectively), return `0` if no build-id exists, and return a negative error code if an
//! unexpected error occurred. This method takes precedence over all other build-id lookup methods
//! (if enabled).
//!
//! ## `buildid-section-inject`
//!
//! When enabled, inject our own symbol into the section where build id is expected to be located,
//! and use the build-time environment variable `BUILD_ID_SIZE` to determine how many bytes to
//! read. This method will only function on some platforms (basically: GNU ones). Note that
//! `BUILD_ID_SIZE` must be set correctly, and differs for GNU ld (bfd) and LLVM lld.
//!
//! Note that in all cases this works, `buildid-symbol-start-end` is likely to work and be more
//! reliable.
//!
//! This method takes precedence over the default lookup methods if enabled.
//!
//! ## `buildid-linker-symbols`
//!
//! When enabled, depend on the `buildid-linker-symbols` crate to automatically create the symbols
//! needed by `buildid-symbol-start-end` on gnu-like linkers.
//!
//! # Platform Details
//!
//!  - On unix variants other than those with apple as the vendor, the `.note.gnu.build-id` is
//!    used. Note that GNU LD and LLD generate different sized build-ids using different hash
//!    functions. Unless additional features are enabled, the `.note.gnu.build-id` is located via
//!    `dl_iterate_phdr()`.
//!  - On Apple unix variants (MacOS), the `LC_UUID` (loader command uuid) is returned directly as
//!    a slice.
//!  - On windows, the module is parsed for a CodeView descriptor containing a GUID (which is
//!    returned directly as a slice). If mingw is used, the same info will appear in the `.buildid`
//!    section, but this lookup method is not used by this library.
//!  - On wasm, no data is provided
//!
//! # Ensuring build-id is enabled
//!
//!  - On windows when using mingw, build-id may not be enabled by default. To enable, set
//!    RUSTFLAGS="-Clink-args=-Wl,--build-id" in the environment before running cargo. The same
//!    argument works for any system using GNU LD or compatible.
//!
//!  - On most linux platforms, build-id is enabled by default by gcc. Sometimes clang on the same
//!    platform does not have build-id enabled though. Set `RUSTFLAGS="-Clink-args=-Wl,--build-id"`
//!    to ensure build id is enabled for clang or gcc
//!
//!  - MacOS appears to enable build-id (LC_UUID) by default, with no change needed.
//!  - Windows MSVC appears to enable build-id (CodeView GUID) by default, with no change needed.
#![no_std]

#[cfg(test)]
extern crate alloc;

mod align;
mod constparse;

cfg_if::cfg_if! {
    if #[cfg(feature = "buildid-custom-inject")] {
        #[path = "custom-inject.rs"]
        mod target;
    } else if  #[cfg(feature = "buildid-section-inject")] {
        #[path = "section-inject.rs"]
        mod target;
    } else if #[cfg(feature = "buildid-symbol-start-end")] {
        #[path = "symbol-start-end.rs"]
        mod target;
    } else if #[cfg(all(
        target_family = "unix",
        not(target_vendor = "apple"),
    ))] {
        #[path = "elf.rs"]
        mod target;
    } else if #[cfg(all(
        target_family = "unix",
        target_vendor = "apple",
    ))] {
        #[path = "mach.rs"]
        mod target;
    } else if #[cfg(target_family = "windows")] {
        #[path = "windows.rs"]
        mod target;
    } else if #[cfg(target_family = "wasm")] {
        mod target {
            pub fn build_id() -> Option<&'static [u8]> {
                // not sure how to implement this right now. need to introspect the wasm object in some way
                None
            }
        }
    }
}

/// If present, return the build-id or platform equivalent
pub fn build_id() -> Option<&'static [u8]> {
    target::build_id()
}

#[cfg(doctest)]
mod test_readme {
    #[doc = include_str!("../README.md")]
    extern "C" {}
}
