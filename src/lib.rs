//! Examine the build-id and similar values for the running executable
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
//!  - On Apple unix variants (MacOS), the `LC_UUID` (loader command uuid) is used
//!  - On windows, the module is parsed for the `.note.gnu.build-id` section
//!  - On wasm, no data is provided

#[cfg(all(target_family = "unix", not(target_vendor = "apple")))]
#[path = "elf.rs"]
mod target;

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

/// If present, return the build-id or platform equivalent
pub fn build_id() -> Option<&'static [u8]> {
    target::build_id()
}
