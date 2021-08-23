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

#![no_std]

#[cfg(all(target_family = "unix", not(target_vendor = "apple")))]
mod target {
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
    #[link_section = ".note.gnu.build-id"]
    static NOTE_GNU_BUILD_ID_END: [u8; 0] = [];

    pub fn build_id() -> Option<&'static [u8]> {
        Ok(unsafe { core::slice::from_raw_parts(NOTE_GNU_BUILD_ID_END.as_ptr().offset(-20), 20) })
    }
}

#[cfg(all(target_family = "unix", target_vendor = "apple"))]
mod target {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    #[repr(C)]
    struct MachHeader {
        /* mach magic number identifier */
        magic: u32,
        /* cpu specifier */
        cpu_type: u32,
        /* machine specifier */
        cpu_subtype: u32,
        /* type of file */
        filetype: u32,
        /* number of load commands */
        ncmds: u32,
        /* the size of all the load commands */
        sizeofcmds: u32,
        /* flags */
        flags: u32,

        #[cfg(target_pointer_width = "64")]
        reserved: u32,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    #[repr(C)]
    struct LoadCommand {
        /* type of load command */
        cmd: u32,
        /* total size of command in bytes */
        cmdsize: u32,
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct CommandIter {
        mh: &'static MachHeader,
        ct: u32,
        lh: Option<&'static LoadCommand>,
    }

    impl CommandIter {
        unsafe fn from_mach_header(mh: &'static MachHeader) -> Self {
            let lh = if mh.ncmds > 0 {
                let loc = (mh as *const MachHeader).offset(1) as *const u8;
                Some(&*(loc as *const LoadCommand))
            } else {
                None
            };
            Self { mh, lh, ct: 0 }
        }

        pub fn new_execute() -> Self {
            unsafe { Self::from_mach_header(&_mh_execute_header) }
        }
    }

    impl Iterator for CommandIter {
        type Item = Command;

        fn next(&mut self) -> Option<Self::Item> {
            if self.ct == self.mh.ncmds {
                return None;
            }

            let lh = match self.lh {
                None => return None,
                Some(lh) => lh,
            };

            self.ct += 1;
            if self.ct == self.mh.ncmds {
                self.lh = None;
            } else {
                let loc = unsafe { (lh as *const _ as *const u8).offset(lh.cmdsize as isize) };
                self.lh = Some(unsafe { &*(loc as *const LoadCommand) });
            }

            let cmd_data_start = unsafe {
                (core::ptr::addr_of!(lh) as *const u8).add(core::mem::size_of::<LoadCommand>())
            };

            Some(Command {
                cmd: lh.cmd,
                data: unsafe {
                    core::slice::from_raw_parts(
                        cmd_data_start,
                        lh.cmdsize as usize - core::mem::size_of::<LoadCommand>(),
                    )
                },
            })
        }
    }

    struct Command {
        cmd: u32,
        data: &'static [u8],
    }

    const LC_UUID: u32 = 0x1b;

    extern "C" {
        static _mh_execute_header: MachHeader;
    }

    // mach-o only
    pub fn build_id() -> Option<&'static [u8]> {
        // _mh_execute_header
        for cmd in CommandIter::new_execute() {
            if cmd.cmd == LC_UUID {
                return Some(cmd.data);
            }
        }

        None
    }
}

#[cfg(target_family = "windows")]
mod target {
    use winapi::um::libloaderapi::GetModuleHandleA;
    use winapi::um::psapi::GetModuleInformation;

    pub fn build_id() -> Option<&'static [u8]> {
        let module = unsafe { GetModuleHandleA(core::ptr::null_mut()) };
        let module_info = core::mem::MaybeUninit::new();

        // NOTE: GetModuleInformation is just using the PE header to get it's info. We can probably
        // skip using it entirely if we grab the size ourselves or `object` can handle it for us.
        let ok = unsafe { GetModuleInformation(core::ptr::null_mut(),
            module, module_info, core::mem::size_of_val(module_info)) };
        if !ok {
            return None;
        }

        let module_info = unsafe { module_info.assume_init() };

        let module_slice = unsafe { core::slice::from_raw_parts(module_info.lpBaseOfDll, module_info.SizeOfImage) };

        // parse the PE file that starts at handle to locate a gnu build-id section or some windows
        // thingy.

        object::
        todo!()
    }
}

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
