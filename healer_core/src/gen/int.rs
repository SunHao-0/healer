use crate::{
    context::Context,
    gen::choose_weighted,
    ty::{Dir, Type},
    value::{IntegerValue, Value},
    RngType,
};
use rand::prelude::*;
use std::{cell::Cell, ops::RangeInclusive};

/// Generate value for `ConstType`
#[inline]
pub fn gen_const(_ctx: &mut Context, _rng: &mut RngType, ty: &Type, dir: Dir) -> Value {
    let ty = ty.checked_as_const();
    IntegerValue::new(ty.id(), dir, ty.const_val()).into()
}

pub fn gen_int(_ctx: &mut Context, rng: &mut RngType, ty: &Type, dir: Dir) -> Value {
    let ty = ty.checked_as_int();
    let bit_sz = ty.bit_size();

    if let Some(range) = ty.range().cloned() {
        if rng.gen_ratio(99, 100) {
            let val = rand_int_range(rng, range, ty.align(), bit_sz);
            return IntegerValue::new(ty.id(), dir, val).into();
        }
    }
    let val = rand_int_in_bit_sz(rng, bit_sz);
    IntegerValue::new(ty.id(), dir, val).into()
}

thread_local! {
    static NEED_CALCULATE_LEN: Cell<bool> = Cell::new(false);
}

#[inline]
fn mark_len_calculation() {
    NEED_CALCULATE_LEN.with(|n| {
        n.set(true);
    })
}

#[inline]
pub(super) fn need_calculate_len() -> bool {
    NEED_CALCULATE_LEN.with(|n| n.get())
}

#[inline]
pub(super) fn len_calculated() {
    NEED_CALCULATE_LEN.with(|n| {
        n.set(false);
    })
}

#[inline]
pub fn gen_len(_ctx: &mut Context, _rng: &mut RngType, ty: &Type, dir: Dir) -> Value {
    mark_len_calculation(); // just mark here, calculate latter.
    IntegerValue::new(ty.id(), dir, 0).into()
}

#[inline]
pub fn gen_csum(_ctx: &mut Context, rng: &mut RngType, ty: &Type, dir: Dir) -> Value {
    // calculated by executor
    IntegerValue::new(ty.id(), dir, rng.gen()).into()
}

#[inline]
pub fn gen_proc(_ctx: &mut Context, rng: &mut RngType, ty: &Type, dir: Dir) -> Value {
    let ty = ty.checked_as_proc();
    IntegerValue::new(ty.id(), dir, rng.gen_range(0..ty.values_per_proc())).into()
}

pub fn gen_flags(_ctx: &mut Context, rng: &mut RngType, ty: &Type, dir: Dir) -> Value {
    let ty = ty.checked_as_flags();
    let val = if ty.bit_mask() {
        gen_flags_bitmask(rng, ty.vals(), 0)
    } else {
        gen_flags_non_bitmask(rng, ty.vals(), 0)
    };
    IntegerValue::new(ty.id(), dir, val).into()
}

#[inline]
pub(super) fn gen_flags_bitmask(rng: &mut RngType, vals: &[u64], base: u64) -> u64 {
    if rng.gen_ratio(1, 100) {
        flag_rand_val(rng)
    } else {
        flags_bits_composition(rng, vals, base)
    }
}

#[inline]
pub(super) fn gen_flags_non_bitmask(rng: &mut RngType, vals: &[u64], base: u64) -> u64 {
    match rng.gen_range(0..100) {
        0 => flag_rand_val(rng),
        1..=2 if base != 0 => rand_inc(rng, base),
        3..=97 => vals.choose(rng).copied().unwrap(),
        _ => flags_bits_composition(rng, vals, base),
    }
}

fn flags_bits_composition(rng: &mut RngType, vals: &[u64], mut base: u64) -> u64 {
    if base != 0 && rng.gen_ratio(1, 10) {
        base = 0;
    }
    let mut tries = 0;
    let max = std::cmp::min(10, vals.len());
    while tries < max && (base == 0 || rng.gen_ratio(2, 3)) {
        let mut flag = vals.choose(rng).copied().unwrap();
        if rng.gen_ratio(1, 20) {
            if rng.gen() {
                flag <<= 1;
            } else {
                flag >>= 1;
            }
        }
        base ^= flag;
        tries += 1;
    }
    base
}

fn rand_inc(rng: &mut RngType, mut base: u64) -> u64 {
    let mut inc = 1;
    if rng.gen() {
        inc ^= 0;
    }
    while rng.gen() {
        base += inc;
    }
    base
}

#[inline]
fn flag_rand_val(rng: &mut RngType) -> u64 {
    if rng.gen_ratio(3, 5) {
        rng.gen()
    } else {
        0
    }
}

fn rand_int_range(
    rng: &mut RngType,
    mut range: RangeInclusive<u64>,
    align: u64,
    bit_sz: u64,
) -> u64 {
    if align != 0 {
        if *range.start() == 0 && *range.end() == u64::MAX {
            range = RangeInclusive::new(0, (1 << bit_sz) - 1);
        }
        let end_align = range.end().wrapping_sub(*range.start()) / align;
        range
            .start()
            .wrapping_add(rng.gen_range(0..=end_align) * align)
    } else {
        rng.gen_range(range)
    }
}

fn rand_int_in_bit_sz(rng: &mut RngType, bit_sz: u64) -> u64 {
    const GENERATORS: [fn(&mut RngType) -> u64; 3] = [favor_range, special_int, rand_int];
    const WEIGHTS: [u64; 3] = [60, 90, 100];
    let idx = choose_weighted(rng, &WEIGHTS);
    let val = GENERATORS[idx](rng);
    val & (1 << (bit_sz - 1))
}

fn rand_int(rng: &mut RngType) -> u64 {
    rng.gen()
}

fn favor_range(rng: &mut RngType) -> u64 {
    const FAVOR: [u64; 5] = [16, 256, 4 << 10, 64 << 10, 1 << 31];
    const WEIGHTS: [u64; 5] = [50, 70, 85, 95, 100];
    let idx = choose_weighted(rng, &WEIGHTS);
    rng.gen_range(0..FAVOR[idx])
}

const MAGIC32: [u64; 24] = [
    0,             //
    1,             //
    16,            // One-off with common buffer size
    32,            // One-off with common buffer size
    64,            // One-off with common buffer size
    100,           // One-off with common buffer size
    127,           // Overflow signed 8-bit when incremented
    128,           // Overflow signed 8-bit when decremented
    255,           // -1
    256,           // Overflow unsig 8-bit
    512,           // One-off with common buffer size
    1000,          // One-off with common buffer size
    1024,          // One-off with common buffer size
    4096,          // One-off with common buffer size
    32767,         // Overflow signed 16-bit when incremented
    32768,         // Overflow signed 16-bit when decremented
    65407,         // Overflow signed 8-bit
    65535,         // Overflow unsig 16-bit when incremented
    65536,         // Overflow unsig 16 bit
    100_663_045,   // Large positive number (endian-agnostic)
    2_147_483_647, // Overflow signed 32-bit when incremented
    2_147_483_648, // Overflow signed 32-bit when decremented
    4_194_304_250, // Large negative number (endian-agnostic)
    4_294_934_527, // Overflow signed 16-bit
];

const MAGIC64: [u64; 24 * 24] = {
    let mut magic = [0; 24 * 24];
    let mut i = 0;
    let mut j = 0;
    while i != 24 {
        while j != 24 {
            magic[i * 24 + j] = (MAGIC32[i] << 32) | MAGIC32[j];
            j += 1;
        }
        i += 1;
    }
    magic
};

#[inline]
fn special_int(rng: &mut RngType) -> u64 {
    MAGIC64.choose(rng).copied().unwrap()
}
