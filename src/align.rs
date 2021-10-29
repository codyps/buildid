/// Return the closest value larger than `value` which is a multiple of `align`
pub fn align_up(value: usize, align: usize) -> usize {
    if align == 0 {
        panic!("align is zero");
    }

    (value + align - 1) / align * align
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn a() {
        assert_eq!(align_up(10, 4), 4 * 3);
    }
}
