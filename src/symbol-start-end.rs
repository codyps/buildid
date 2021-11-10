extern "C" {
    static __build_id_start: [u8; 1];
    static __build_id_end: [u8; 1];
}

pub fn build_id() -> Option<&'static [u8]> {
    unsafe {
        let start = __build_id_start.as_ptr();
        let end = __build_id_end.as_ptr();
        let len = end.offset_from(start).try_into().unwrap();
        Some(core::slice::from_raw_parts(start, len))
    }
}
