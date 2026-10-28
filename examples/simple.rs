fn main() {
    env_logger::init();
    println!("build-id: {:X?}", buildid::build_id())
}

#[cfg(target_family = "wasm")]
#[no_mangle]
pub extern "C" fn build_id_ptr() -> *const u8 {
    buildid::build_id().map_or(core::ptr::null(), <[u8]>::as_ptr)
}

#[cfg(target_family = "wasm")]
#[no_mangle]
pub extern "C" fn build_id_len() -> usize {
    buildid::build_id().map_or(0, <[u8]>::len)
}
