/// Return the closest value larger than `value` which is a multiple of `align`
pub fn align_up(value: usize, align: usize) -> usize {
    assert_ne!(align, 0, "align is zero");

    value.div_ceil(align) * align
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn a() {
        assert_eq!(align_up(10, 4), 4 * 3);
    }
}
