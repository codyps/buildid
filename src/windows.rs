
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

    // object::
    todo!()
}
