use crate::align::align_up;
use std::convert::TryInto;
use std::mem;
use std::mem::MaybeUninit;
use thiserror::Error;
use tracing::{event, Level};

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
/*
// This method only works if a build-id of exactly the right size is linked in. Otherwise, the
// link fails or invalid data is accessed
#[link_section = ".note.gnu.build-id"]
static NOTE_GNU_BUILD_ID_END: [u8; 0] = [];

const BUILD_ID_LEN: usize = 20;

pub fn build_id() -> Option<&'static [u8]> {
    Some(unsafe { core::slice::from_raw_parts(NOTE_GNU_BUILD_ID_END.as_ptr().sub(BUILD_ID_LEN), BUILD_ID_LEN) })
}
*/

// FIXME: dl_phdr_info references are actually unsafe here because of how glibc defines
// dl_phdr_info (to have some fields present depending on the size provided (<glibc-2.4 omits
// them).
//
// We'll probably get away with this because we're not accessing those fields, but this is
// technically wrong because we can access those fields in safe code.
//

#[cfg(target_pointer_width = "64")]
mod elf {
    pub type ElfPhdr = libc::Elf64_Phdr;
}
#[cfg(target_pointer_width = "32")]
mod elf {
    pub type ElfPhdr = libc::Elf32_Phdr;
}
use elf::*;

const NT_GNU_BUILD_ID: u32 = 3;

// TODO: experiment with using more custom DST here to avoid manually extracting fields
#[derive(Debug)]
struct Note {
    data: [u8],
}

const MIN_NOTE_SIZE: usize = mem::size_of::<usize>() * 3;

#[derive(Debug, Error)]
enum NoteError {
    #[error("have {size} bytes, but need at least {MIN_NOTE_SIZE}")]
    MissingHeader { size: usize },

    #[error("have {have} bytes, but need at least {need}")]
    Truncated { have: usize, need: usize },
}

impl Note {
    // NOTE: the _standards_ say to use 8 byte alignment in 64-bit land. But llvm and others note
    // that everyone actually uses 4 byte alignment. Perfect. Hopefully this always works.
    const ALIGN: usize = 4;

    // technically safe because we'll only panic if I screw up
    fn from_bytes_raw(data: &[u8]) -> &Self {
        unsafe { &*(data as *const [u8] as *const Note) }
    }

    fn from_bytes(data: &[u8]) -> Result<(&Self, &[u8]), NoteError> {
        let u = std::mem::size_of::<u32>();
        if data.len() < u * 3 {
            return Err(NoteError::MissingHeader { size: data.len() });
        }

        Self::from_bytes_raw(data).split_trailing()
    }

    fn name_len(&self) -> usize {
        u32::from_ne_bytes(self.data[..std::mem::size_of::<u32>()].try_into().unwrap()) as usize
    }

    fn desc_len(&self) -> usize {
        let u = std::mem::size_of::<u32>();
        u32::from_ne_bytes(self.data[u..(u + u)].try_into().unwrap()) as usize
    }

    fn type_(&self) -> u32 {
        let u = std::mem::size_of::<u32>();
        u32::from_ne_bytes(self.data[(u + u)..(u + u + u)].try_into().unwrap())
    }

    fn name(&self) -> &[u8] {
        let u = std::mem::size_of::<u32>();
        let b = u * 3;
        &self.data[b..(b + self.name_len())]
    }

    fn desc(&self) -> &[u8] {
        let u = std::mem::size_of::<u32>();
        let b = u * 3 + align_up(self.name_len(), Self::ALIGN);
        &self.data[b..(b + self.desc_len())]
    }

    fn split_trailing(&self) -> Result<(&Self, &[u8]), NoteError> {
        let u = std::mem::size_of::<u32>();
        let end =
            u * 3 + align_up(self.name_len(), Self::ALIGN) + align_up(self.desc_len(), Self::ALIGN);
        if end > self.data.len() {
            Err(NoteError::Truncated {
                need: end,
                have: self.data.len(),
            })
        } else {
            Ok((Self::from_bytes_raw(&self.data[0..end]), &self.data[end..]))
        }
    }
}

// Ideally, we'd use a trait alias instead of a type alias and construct the type out of the
// trait. But that's not stable right now (see https://github.com/rust-lang/rust/issues/41517)
unsafe extern "C" fn phdr_cb(
    info: *mut libc::dl_phdr_info,
    size: libc::size_t,
    data: *mut libc::c_void,
) -> libc::c_int {
    let closure: &mut &mut dyn FnMut(&libc::dl_phdr_info, usize) -> libc::c_int =
        &mut *(data as *mut &mut dyn for<'r> std::ops::FnMut(&'r libc::dl_phdr_info, usize) -> i32);
    let info = &*info;

    closure(info, size)
}

fn object_map<F: FnMut(&'static libc::dl_phdr_info, usize) -> libc::c_int>(
    mut cb: F,
) -> libc::c_int {
    let mut cb: &mut dyn FnMut(&'static libc::dl_phdr_info, usize) -> libc::c_int = &mut cb;
    let cb = &mut cb;
    unsafe { libc::dl_iterate_phdr(Some(phdr_cb), cb as *mut _ as *mut _) }
}

/// Iterate over program headers described by a dl_phdr_info
struct PhdrIter<'a> {
    info: &'a libc::dl_phdr_info,
    i: u16,
}

impl<'a> Iterator for PhdrIter<'a> {
    type Item = &'a libc::Elf64_Phdr;

    fn next(&mut self) -> Option<Self::Item> {
        if self.i >= self.info.dlpi_phnum {
            return None;
        }

        let phdr = unsafe { &*self.info.dlpi_phdr.add(self.i as usize) };
        self.i += 1;
        Some(phdr)
    }
}

impl<'a> From<&'a libc::dl_phdr_info> for PhdrIter<'a> {
    fn from(info: &'a libc::dl_phdr_info) -> Self {
        PhdrIter { info, i: 0 }
    }
}

/// Iterate over notes stored in a PT_NOTE program section
#[derive(Debug)]
struct NoteIter<'a> {
    segment: &'a [u8],
}

impl<'a> NoteIter<'a> {
    fn new(info: &'a libc::dl_phdr_info, phdr: &'a ElfPhdr) -> Option<Self> {
        // NOTE: each dl_phdr_info describes multiple program segments. In this iterator, we're
        // only examining one of them.
        //
        // We'll also need to iterate over program segments to pick out the PT_NOTE one we need.

        if phdr.p_type != libc::PT_NOTE {
            None
        } else {
            let segment_base = (info.dlpi_addr + phdr.p_vaddr) as *const u8;
            let segment = unsafe {
                // FIXME: consider p_memsz vs p_filesz question here.
                // llvm appears to use filesz
                std::slice::from_raw_parts(segment_base, phdr.p_filesz as usize)
            };
            Some(NoteIter { segment })
        }
    }
}

impl<'a> Iterator for NoteIter<'a> {
    type Item = Result<&'a Note, NoteError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.segment.is_empty() {
            return None;
        }

        let (n, r) = match Note::from_bytes(self.segment) {
            Err(e) => return Some(Err(e)),
            Ok(v) => v,
        };

        self.segment = r;

        Some(Ok(n))
    }
}

pub fn build_id() -> Option<&'static [u8]> {
    // find the shared object that contains our own `build_id()` fn (this fn itself)
    let data = {
        let mut data = MaybeUninit::uninit();
        if unsafe { libc::dladdr(&build_id as *const _ as *mut _, data.as_mut_ptr()) } == 0 {
            // TODO: consider if we have fallback options here
            return None;
        }

        unsafe { data.assume_init() }
    };

    let mut res = None;
    // FIXME: we probably should avoid ignoring `size` here so we can bounds check our
    // accesses. Basically need to treat this data as a big array we happen to have pointers
    // into, and convert those pointers to offsets.
    object_map(|info, _size| {
        let mut map_start = None;

        for phdr in PhdrIter::from(info) {
            // FIXME: unclear why the first PT_LOAD is the right thing to use here
            if phdr.p_type == libc::PT_LOAD {
                map_start = Some(info.dlpi_addr + phdr.p_vaddr);
                break;
            }
        }

        let map_start = match map_start {
            Some(v) => v,
            None => return -1,
        };

        // check if this phdr is the one that contains our fn.
        if map_start != data.dli_fbase as u64 {
            return -1;
        }

        'phdr: for phdr in PhdrIter::from(info) {
            let ni = match NoteIter::new(info, phdr) {
                Some(v) => v,
                None => continue,
            };

            // iterate over notes
            for note in ni {
                let note = match note {
                    Err(e) => {
                        event!(Level::ERROR, "note program segment had invalid note {}", e);
                        continue 'phdr;
                    }
                    Ok(v) => v,
                };
                if note.type_() == NT_GNU_BUILD_ID
                    && !note.desc().is_empty()
                    && note.name() == b"GNU\0"
                {
                    res = Some(note.desc());
                    break 'phdr;
                }
            }
        }

        0
    });

    res
}
