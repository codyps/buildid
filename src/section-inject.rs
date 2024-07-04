// NOTE: unix doesn't necessarily promise we'll have this section. We can use some functions to
// dynamically look it up instead if we have issues.
//
// NOTE: this build id does not include any dynamically linked libraries. We can get those
// build ids seperately by performing some dynamic lookups.
//
// NOTE: this works by adding a zero sized symbol to the end of the build-id section, but it's
// not entirely clear why we're always at the end of the build-id section (instead of at the
// beginning). We don't have any way to measure the size of the section, so we just have to
// assume the hash size based on what is currently in common use (which is a 20 byte/160 bit
// hash as I'm writing this).
//
// NOTE: current gcc enables build-id by default, but current clang does not. To use clang,
// ensure one does `RUSTFLAGS='-C linker=clang -Clink-arg=-Wl,--build-id'` or similar.
//
// NOTE: If using a toolchain without build-id enabled, junk is returned (likely the content of
// other note sections). We could do a small bit of validation by checking the note header.
//
// This method only works if a build-id of exactly the right size is linked in. Otherwise, the
// link fails or invalid data is accessed
#[link_section = ".note.gnu.build-id"]
static NOTE_GNU_BUILD_ID_END: [u8; 0] = [];

// 20 for GNU
const BUILD_ID_LEN: usize = crate::constparse::parse_usize(env!("BUILD_ID_LEN"));

pub fn build_id() -> Option<&'static [u8]> {
    Some(unsafe {
        core::slice::from_raw_parts(
            NOTE_GNU_BUILD_ID_END.as_ptr().sub(BUILD_ID_LEN),
            BUILD_ID_LEN,
        )
    })
}
