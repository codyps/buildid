// Copyright (c) 2021 DutchGhost
// Copyright (c) 2021 matrixmultiply authors
// Copyright 2024 Cody Schafer
//
// Adapted from
// https://github.com/bluss/matrixmultiply/blob/c7ab1aca8ac0ac4c4b09382ff515f4ab5f0d3ec1/src/constparse.rs,
// which was in turn adapted from
// https://gist.github.com/DutchGhost/d8604a3c796479777fe9f5e25d855cfd.
//
// Further modified here to take advantage of const fn improvements in Rust since those were
// published.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[derive(Clone, Copy, Debug)]
pub(crate) enum ParseIntError {
    InvalidDigit,
}

const fn parse_byte(b: u8, pow10: usize) -> Result<usize, ParseIntError> {
    let r = b.wrapping_sub(48);

    if r > 9 {
        Err(ParseIntError::InvalidDigit)
    } else {
        Ok((r as usize) * pow10)
    }
}

const POW10: [usize; 20] = {
    let mut array = [0; 20];
    let mut pow10 = 1usize;

    let mut index = 20;

    loop {
        index -= 1;
        array[index] = pow10;

        if index == 0 {
            break;
        }

        let (new_power, overflow) = pow10.overflowing_mul(10);
        pow10 = new_power;
        if overflow {
            break;
        }
    }

    array
};

const fn try_parse_usize(b: &str) -> Result<usize, ParseIntError> {
    let bytes = b.as_bytes();

    let mut result: usize = 0;

    let len = bytes.len();

    // Start at the correct index of the table,
    // (skip the power's that are too large)
    let mut index_const_table = POW10.len().wrapping_sub(len);
    let mut index = 0;

    while index < b.len() {
        let a = bytes[index];
        let p = POW10[index_const_table];

        let r = match parse_byte(a, p) {
            Err(e) => return Err(e),
            Ok(d) => d,
        };

        result = result.wrapping_add(r);

        index += 1;
        index_const_table += 1;
    }

    Ok(result)
}

pub(crate) const fn parse_usize(b: &str) -> usize {
    match try_parse_usize(b) {
        Ok(v) => v,
        Err(_) => panic!("could not parse usize"),
    }
}

#[test]
fn test_parse() {
    use alloc::string::ToString;
    for i in 0..500 {
        assert_eq!(parse_usize(&i.to_string()), i);
    }

    for bits in 0usize..core::mem::size_of::<usize>() * 8 {
        let i = (1 << bits) - 1;
        assert_eq!(parse_usize(&i.to_string()), i);
    }
}
