
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
