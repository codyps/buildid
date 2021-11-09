use core::mem::MaybeUninit;

extern "C" {
    fn build_id__get(build_id: *mut *const u8, len: *mut usize) -> cty::c_int;
}

pub fn build_id() -> Option<&'static [u8]> {
    let mut b = MaybeUninit::<*const u8>::uninit();
    let mut l = MaybeUninit::<usize>::uninit();
    let r = unsafe { build_id__get(b.as_mut_ptr(), l.as_mut_ptr()) };

    if r < 0 {
        log::error!("build_id__get returned error: {}", r);
        None
    } else if r == 0 {
        None
    } else {
        let b = unsafe { b.assume_init() };
        let l = unsafe { l.assume_init() };

        Some(unsafe { core::slice::from_raw_parts(b, l) })
    }
}
