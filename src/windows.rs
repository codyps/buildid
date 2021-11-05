use lazy_static::lazy_static;
use tracing::{event, Level};
use winapi::um::dbghelp::ImageNtHeader;
use winapi::um::libloaderapi::GetModuleHandleA;
use winapi::um::winnt::IMAGE_DEBUG_DIRECTORY;
use winapi::um::winnt::IMAGE_DEBUG_TYPE_CODEVIEW;
use winapi::um::winnt::IMAGE_DIRECTORY_ENTRY_DEBUG;

#[allow(bad_style)]
struct CV_INFO_PDB70 {
    cv_signature: u32,
    signature: [u8; 16],
    _age: u32,
    // followed by pdb name
}

lazy_static! {
    // This primarily exists as a hack to allow us to return a `&'static [u8]`
    static ref BUILD_ID_CACHE: Option<&'static [u8]> = {
        let module = unsafe { GetModuleHandleA(core::ptr::null_mut()) };
        event!(Level::TRACE, "module {:#x}", module as usize);

        let image_nt_header = unsafe { &*ImageNtHeader(module as _) };

        let opt_header = &image_nt_header.OptionalHeader;
        let dir = &opt_header.DataDirectory[IMAGE_DIRECTORY_ENTRY_DEBUG as usize];
        if dir.Size == 0 {
            event!(Level::ERROR, "IMAGE_DIRECTORY_ENTRY_DEBUG is empty");
            return None;
        }

        let dbg_dir = unsafe { &*((module as usize + dir.VirtualAddress as usize) as *const IMAGE_DEBUG_DIRECTORY) };

        if dbg_dir.Type == IMAGE_DEBUG_TYPE_CODEVIEW {
            let pdb_info = unsafe { &*((module as usize + dbg_dir.AddressOfRawData as usize) as *const CV_INFO_PDB70) };
            if pdb_info.cv_signature != 0x53445352 {
                event!(Level::ERROR, "mismatch sig: {:#x}", pdb_info.cv_signature);
                None
            } else {
                Some(&pdb_info.signature[..])
            }
        } else {
            event!(Level::ERROR, "wrong image type {:#x}", dbg_dir.Type);
            None
        }
    };
}

pub fn build_id() -> Option<&'static [u8]> {
    *BUILD_ID_CACHE
}
