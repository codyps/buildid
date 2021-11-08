use log::error;
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

pub fn build_id() -> Option<&'static [u8]> {
    let module = unsafe { GetModuleHandleA(core::ptr::null_mut()) };

    let image_nt_header = unsafe { &*ImageNtHeader(module as _) };

    let opt_header = &image_nt_header.OptionalHeader;
    let dir = &opt_header.DataDirectory[IMAGE_DIRECTORY_ENTRY_DEBUG as usize];
    if dir.Size == 0 {
        error!("IMAGE_DIRECTORY_ENTRY_DEBUG is empty");
        return None;
    }

    let dbg_dir = unsafe {
        &*((module as usize + dir.VirtualAddress as usize) as *const IMAGE_DEBUG_DIRECTORY)
    };

    if dbg_dir.Type == IMAGE_DEBUG_TYPE_CODEVIEW {
        let pdb_info = unsafe {
            &*((module as usize + dbg_dir.AddressOfRawData as usize) as *const CV_INFO_PDB70)
        };
        if pdb_info.cv_signature != 0x53445352 {
            error!("mismatch sig: {:#x}", pdb_info.cv_signature);
            None
        } else {
            Some(&pdb_info.signature[..])
        }
    } else {
        error!("wrong image type {:#x}", dbg_dir.Type);
        None
    }
}
