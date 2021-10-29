use lazy_static::lazy_static;
use object::Object;
use tracing::{event, Level};
use winapi::um::libloaderapi::GetModuleHandleA;
use winapi::um::psapi::GetModuleInformation;

lazy_static! {
    // This primarily exists as a hack to allow us to return a `&'static [u8]`
    static ref BUILD_ID_CACHE: Option<&'static [u8]> = {
        let module = unsafe { GetModuleHandleA(core::ptr::null_mut()) };
        let mut module_info = core::mem::MaybeUninit::uninit();

        // NOTE: GetModuleInformation is just using the PE header to get it's info. We can probably
        // skip using it entirely if we grab the size ourselves or `object` can handle it for us.
        let ok = unsafe {
            GetModuleInformation(
                core::ptr::null_mut(),
                module,
                module_info.as_mut_ptr(),
                core::mem::size_of_val(&module_info) as u32,
            )
        };
        if ok != 0 {
            return None;
        }

        let module_info = unsafe { module_info.assume_init() };

        let module_slice: &'static [u8] = unsafe {
            core::slice::from_raw_parts(
                module_info.lpBaseOfDll as *const u8,
                module_info.SizeOfImage as usize,
            )
        };

        // parse the PE file that starts at handle to locate a gnu build-id section or some windows
        // thingy.

        // let v = match object::read::File::parse(module_slice) {
        let v: object::read::pe::PeFile<'_, object::pe::ImageNtHeaders64> = match object::read::pe::PeFile::parse(module_slice) {
            Err(e) => {
                event!(Level::ERROR, "module parse failed: {}", e);
                return None;
            }
            Ok(v) => v,
        };

        let cv = match v.pdb_info() {
            Err(e) => {
                event!(Level::ERROR, "error obtaining CodeView: {}", e);
                return None;
            }
            Ok(None) => {
                event!(Level::INFO, "no pdb_info present in executable");
                return None;
            }
            Ok(Some(v)) => v,
        };

        Some(&Box::leak(Box::new(cv.guid()))[..])
    };
}

pub fn build_id() -> Option<&'static [u8]> {
    *BUILD_ID_CACHE
}
