//! Examine the build-id and similar values for the running executable
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
//! # Platform Details
//!
//!  - On unix variants other than those with apple as the vendor, the `.note.gnu.build-id` is used
//!    and returned as a slice. Note that GNU LD and LLD generate different sized build-ids using
//!    different hash functions.
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

#[cfg(all(target_family = "unix", not(target_vendor = "apple")))]
#[path = "elf.rs"]
mod target;

#[cfg(all(target_family = "unix", not(target_vendor = "apple")))]
mod align;

#[cfg(all(target_family = "unix", target_vendor = "apple"))]
#[path = "mach.rs"]
mod target;

#[cfg(target_family = "windows")]
#[path = "windows.rs"]
mod target;

#[cfg(target_family = "wasm")]
mod target {
    pub fn build_id() -> Option<&'static [u8]> {
        // not sure how to implement this right now. need to introspect the wasm object in some way
        None
    }
}

// TODO: provide a feature that allows using known symbols
// TODO: provide a feature that allows injecting a symbol into a particular section

/// If present, return the build-id or platform equivalent
pub fn build_id() -> Option<&'static [u8]> {
    target::build_id()
}
