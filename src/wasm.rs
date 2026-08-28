use core::cell::UnsafeCell;

use crate::wasm_stamp::{
    ID_LEN, MODE_HASH, MODE_TOP_LEVEL, MODE_UNSTAMPED, SLOT_PREFIX, SLOT_SUFFIX, UNSTAMPED_ID,
    UNSTAMPED_LEN,
};

#[repr(C)]
struct BuildIdSlot {
    prefix: [u8; SLOT_PREFIX.len()],
    mode: u8,
    len: u8,
    id: [u8; ID_LEN],
    suffix: [u8; SLOT_SUFFIX.len()],
}

#[repr(transparent)]
struct SlotCell(UnsafeCell<BuildIdSlot>);

// The slot is only modified in the Wasm file before instantiation. Keeping it
// in an UnsafeCell prevents LLVM/LTO from replacing reads with the known
// unstamped initializer that the stamping tool replaces after linking.
unsafe impl Sync for SlotCell {}

#[used]
static BUILD_ID_SLOT: SlotCell = SlotCell(UnsafeCell::new(BuildIdSlot {
    prefix: *SLOT_PREFIX,
    mode: MODE_UNSTAMPED,
    len: UNSTAMPED_LEN,
    id: *UNSTAMPED_ID,
    suffix: *SLOT_SUFFIX,
}));

pub fn build_id() -> Option<&'static [u8]> {
    let slot = unsafe { &*BUILD_ID_SLOT.0.get() };

    let len = usize::from(slot.len);
    if slot.prefix != *SLOT_PREFIX
        || slot.suffix != *SLOT_SUFFIX
        || !matches!(slot.mode, MODE_HASH | MODE_TOP_LEVEL)
        || len == 0
        || len > ID_LEN
    {
        None
    } else {
        Some(&slot.id[..len])
    }
}
