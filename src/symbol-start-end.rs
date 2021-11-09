extern "C" {
    static __build_id_start: u8;
    static __build_id_end: u8;
}

pub fn build_id() -> Option<&'static [u8]> {
    unsafe {
        let start = (&__build_id_start) as *const u8;
        let end = (&__build_id_end) as *const u8;
        let l = end.offset_from(start).try_into().unwrap();
        Some(core::slice::from_raw_parts(start, l))
    }
}
