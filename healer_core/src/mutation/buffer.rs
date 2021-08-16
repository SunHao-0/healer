//! Mutate value of `blob`, `string`, `filename` type.
use crate::ty::Dir;
use crate::{context::Context, value::Value, RngType};
use rand::prelude::*;
use rand::Rng;
use std::cmp::min;
use std::convert::TryFrom;
use std::ops::{Range, RangeInclusive};

pub const MAX_BLOB_LEN: u64 = 100 << 10;

/// Mutate a buffer value
pub fn mutate_blob(ctx: &mut Context, rng: &mut RngType, val: &mut Value) {
    let ty = val.ty(ctx.target()).checked_as_buffer_blob();
    let r = ty.range().unwrap_or(RangeInclusive::new(0, MAX_BLOB_LEN));
    let val = val.checked_as_data_mut();

    if val.dir() == Dir::Out {
        val.size = mutate_blob_len(rng, val.size, r);
    } else {
        do_mutate_blob(ctx, rng, &mut val.data, r)
    }
}

/// Inc or dec the size `old` with little step.
fn mutate_blob_len(rng: &mut RngType, old: u64, r: RangeInclusive<u64>) -> u64 {
    let mut new = if rng.gen() {
        let delta = rng.gen_range(0..=old);
        old - delta
    } else {
        let delta = rng.gen_range(0..=16);
        old + delta
    };
    if new < *r.start() {
        new = *r.start();
    } else if new > *r.end() {
        new = *r.end();
    }
    new
}

// A blob mutating opertaion tries to mutate the input buffer, return `true` if it successed.
pub type BlobMutateOperation =
    fn(&mut Context, &mut RngType, &mut Vec<u8>, RangeInclusive<u64>) -> bool;

/// All the supported blob mutatation operations, mainly including block level
/// operations and scalar level operations.
pub const BLOB_MUTATE_OPERATIONS: [BlobMutateOperation; 27] = [
    // block level operations
    shuffle,
    erase,
    overwrite_within,
    overwrite_random,
    insert_within,
    insert_0x00,
    insert_0xff,
    insert_one_special,
    insert_one_random,
    insert_random,
    // scalar level operations
    set_length8,
    perturb8,
    replace_magic8,
    set_length16,
    perturb16,
    replace_magic16,
    set_length32,
    perturb32,
    replace_magic32,
    set_length64,
    perturb64,
    insert8,
    insert_special8,
    modify1,
    modify8,
    modify_special8,
    modify_ascii_int,
];

#[inline]
fn do_mutate_blob(ctx: &mut Context, rng: &mut RngType, buf: &mut Vec<u8>, r: RangeInclusive<u64>) {
    let mut mutated = false;
    let mut tries = 0;

    while tries < 128 && (!mutated || rng.gen_ratio(1, 3)) {
        let op = BLOB_MUTATE_OPERATIONS.choose(rng).unwrap();
        mutated = op(ctx, rng, buf, r.clone());
        tries += 1;
    }

    if !r.contains(&(buf.len() as u64)) && rng.gen_ratio(99, 100) {
        fixup_blob_length(rng, buf, r)
    }
}

#[inline]
fn fixup_blob_length(rng: &mut RngType, buf: &mut Vec<u8>, r: RangeInclusive<u64>) {
    let start = *r.start() as usize;
    let end = *r.end() as usize;
    if buf.len() < start {
        buf.resize_with(start, || rng.gen())
    } else if buf.len() > end {
        buf.truncate(end)
    }
}

/// Choose the length of bytes to be mutated, the buffer itself must longger than 2.
#[inline]
fn rand_op_len(rng: &mut RngType, len: usize) -> usize {
    debug_assert!(len >= 2);

    let max = min(len, 32); // make this small
    rng.gen_range(2..=max)
}

/// Choose the range of bytes to be mutated, the buffer itself must be longger than 2.
#[inline]
fn rand_op_range(rng: &mut RngType, len: usize) -> Range<usize> {
    let op_len = rand_op_len(rng, len);
    let start = rng.gen_range(0..=(len - op_len));
    start..(start + op_len)
}

/// Shuffles the bytes.
fn shuffle(
    _ctx: &mut Context,
    rng: &mut RngType,
    buf: &mut Vec<u8>,
    _r: RangeInclusive<u64>,
) -> bool {
    if buf.len() < 2 {
        return false;
    }
    let r = rand_op_range(rng, buf.len());
    buf[r].shuffle(rng);
    true
}

/// Erases a block.
fn erase(_ctx: &mut Context, rng: &mut RngType, buf: &mut Vec<u8>, r: RangeInclusive<u64>) -> bool {
    let max_len = buf.len().saturating_sub(*r.start() as usize);
    if max_len < 2 {
        return false;
    }
    let r = rand_op_range(rng, max_len);
    buf.drain(r);
    true
}

/// Overwrites a block with a random block within the buffer.
fn overwrite_within(
    _ctx: &mut Context,
    rng: &mut RngType,
    buf: &mut Vec<u8>,
    _r: RangeInclusive<u64>,
) -> bool {
    let len = buf.len();
    if len < 2 {
        return false;
    }
    let op_len = rand_op_len(rng, len);
    let src = rng.gen_range(0..=(len - op_len));
    let dst = rng.gen_range(0..=(len - op_len));
    if op_len != len && src != dst {
        // SAFETY: `buf`, `src_start` and `dst_start` are valid, `op_len` won't exceed required range.
        unsafe { std::ptr::copy(&buf[src], &mut buf[dst], op_len) };
        return true;
    }
    false
}

/// Overwrites a block with random bytes.
fn overwrite_random(
    _ctx: &mut Context,
    rng: &mut RngType,
    buf: &mut Vec<u8>,
    _r: RangeInclusive<u64>,
) -> bool {
    let len = buf.len();
    if len < 2 {
        return false;
    }
    let r = rand_op_range(rng, len);
    rng.fill(&mut buf[r]);
    true
}

/// Duplicates a block and inserts it at random position.
fn insert_within(
    _ctx: &mut Context,
    rng: &mut RngType,
    buf: &mut Vec<u8>,
    r: RangeInclusive<u64>,
) -> bool {
    let max_len = min(buf.len(), (*r.end() as usize).saturating_sub(buf.len()));
    if max_len < 2 {
        return false;
    }
    let op_len = rand_op_len(rng, max_len);
    let src = rng.gen_range(0..=(buf.len() - op_len));
    let dst = rng.gen_range(0..=buf.len());

    let src_backup = buf[src..(src + op_len)].to_vec();
    buf.splice(dst..dst, src_backup);
    true
}

/// Inserts content by generating new bytes.
fn insert_new_content(
    rng: &mut RngType,
    buf: &mut Vec<u8>,
    r: RangeInclusive<u64>,
    mut modify: impl FnMut(&mut RngType, &mut u8),
) -> bool {
    let max_len = (*r.end() as usize).saturating_sub(buf.len());
    if max_len < 2 {
        return false;
    }
    let len = rand_op_len(rng, max_len);
    let start = rng.gen_range(0..=buf.len());
    let prev_end = buf.len();
    buf.reserve(len);
    unsafe { buf.set_len(len + prev_end) };
    buf.copy_within(start..prev_end, start + len);
    for i in &mut buf[start..start + len] {
        modify(rng, i);
    }
    true
}

/// Inserts a block composed of 0x00.
fn insert_0x00(
    _ctx: &mut Context,
    rng: &mut RngType,
    buf: &mut Vec<u8>,
    r: RangeInclusive<u64>,
) -> bool {
    insert_new_content(rng, buf, r, |_, i| *i = 0)
}

/// Inserts a block composed of 0xff.
fn insert_0xff(
    _ctx: &mut Context,
    rng: &mut RngType,
    buf: &mut Vec<u8>,
    r: RangeInclusive<u64>,
) -> bool {
    insert_new_content(rng, buf, r, |_, i| *i = 0xff)
}

const SPECIAL_BYTE: &[u8] = b"!*'();:@&=+$,/?%#[]012Az-`~.\xff\x00";

/// Inserts a block composed of repetitions of *one* random special bytes.
fn insert_one_special(
    _ctx: &mut Context,
    rng: &mut RngType,
    buf: &mut Vec<u8>,
    r: RangeInclusive<u64>,
) -> bool {
    let byte = *SPECIAL_BYTE.choose(rng).unwrap();
    insert_new_content(rng, buf, r, |_, v| *v = byte)
}

/// Inserts a block composed of repetitions of *one* random bytes.
fn insert_one_random(
    _ctx: &mut Context,
    rng: &mut RngType,
    buf: &mut Vec<u8>,
    r: RangeInclusive<u64>,
) -> bool {
    let byte = rng.gen();
    insert_new_content(rng, buf, r, |_, v| *v = byte)
}

/// Inserts a block composed of random bytes.
fn insert_random(
    _ctx: &mut Context,
    rng: &mut RngType,
    buf: &mut Vec<u8>,
    r: RangeInclusive<u64>,
) -> bool {
    insert_new_content(rng, buf, r, |rng, v| *v = rng.gen())
}

/// These operations come from `quaff`

trait Number: Sized {
    const LEN: usize;
    fn read(buf: &[u8], swap: bool) -> Self;
    fn write(self, buf: &mut [u8], swap: bool);
    fn overflowing_add(self, rhs: Self) -> (Self, bool);
    fn overflowing_neg(self) -> (Self, bool);
}

macro_rules! impl_num_trait {
    ($ty:ty) => {
        impl Number for $ty {
            const LEN: usize = std::mem::size_of::<$ty>();
            fn read(buf: &[u8], swap: bool) -> Self {
                let mut array = [0u8; Self::LEN];
                array[..].copy_from_slice(&buf[..Self::LEN]);
                let v = <$ty>::from_ne_bytes(array);
                if swap {
                    v.swap_bytes()
                } else {
                    v
                }
            }

            fn write(self, buf: &mut [u8], swap: bool) {
                let new = if swap { self.swap_bytes() } else { self };
                let bytes = new.to_ne_bytes();
                buf[..Self::LEN].copy_from_slice(&bytes);
            }

            fn overflowing_add(self, rhs: Self) -> (Self, bool) {
                self.overflowing_add(rhs)
            }

            fn overflowing_neg(self) -> (Self, bool) {
                self.overflowing_neg()
            }
        }
    };
}

impl_num_trait!(u8);
impl_num_trait!(u16);
impl_num_trait!(u32);
impl_num_trait!(u64);

macro_rules! impl_scalar {
    ($ty:ty => $header:ident, $arith:ident $(, $magic:ident = $magic_values:expr)?) => {
        /// Replaces one scalar of the first 64 bytes with buffer's length.
        fn $header(_ctx: &mut Context, rng: &mut RngType, buf: &mut Vec<u8>, _r: RangeInclusive<u64>) -> bool {
            if buf.len() < <$ty>::LEN {
                return false;
            }
            let swap = rng.gen_ratio(1, 2);
            let pos_max = min(buf.len() - <$ty>::LEN + 1, 64);
            let pos = rng.gen_range(0..pos_max);
            let new = if let Ok(val) = <$ty>::try_from(buf.len()) {val} else {return false};
            new.write(&mut buf[pos..], swap);
            true
        }

        /// Perturbs one scalar by adding [-35, 0) ∪ (0, +35] to it or flipping
        /// the sign.
        fn $arith(_ctx: &mut Context, rng: &mut RngType, buf: &mut Vec<u8>, _r: RangeInclusive<u64>) -> bool {
            const RANGE: u8 = 35;

            if buf.len() < <$ty>::LEN {
                return false;
            }
            let swap = rng.gen_ratio(1, 2);
            let pos = rng.gen_range(0..buf.len() - <$ty>::LEN + 1);
            let new = <$ty>::read(&buf[pos..], swap);
            let range = RANGE as $ty * 2 + 1;
            let offset = rng.gen_range(0..range).overflowing_sub(RANGE as $ty).0;
            let new = if offset == 0 {
                new.overflowing_neg().0
            } else {
                new.overflowing_add(offset).0
            };
            new.write(&mut buf[pos..], swap);
            true
        }

        $(
        /// Replaces one scalar with a predefined magic value.
        fn $magic(_ctx: &mut Context, rng: &mut RngType, buf: &mut Vec<u8>, _r: RangeInclusive<u64>) -> bool {
            const MAGIC: &'static [$ty] = &$magic_values;

            if buf.len() < <$ty>::LEN {
                return false;
            }
            let swap = rng.gen_ratio(1, 2);
            let pos = rng.gen_range(0..buf.len() - <$ty>::LEN + 1);
            let new = MAGIC.choose(rng).unwrap();
            new.write(&mut buf[pos..], swap);
            true
        }
        )*
    };
}

impl_scalar!(u8 => set_length8, perturb8, replace_magic8 = [
    0,   //
    1,   //
    16,  // One-off with common buffer size
    32,  // One-off with common buffer size
    64,  // One-off with common buffer size
    100, // One-off with common buffer size
    127, // Overflow signed 8-bit when incremented
    128, // Overflow signed 8-bit when decremented
    255, // -1
]);

impl_scalar!(u16 => set_length16, perturb16, replace_magic16 = [
    128,   // Overflow signed 8-bit
    255,   // Overflow unsig 8-bit when incremented
    256,   // Overflow unsig 8-bit
    512,   // One-off with common buffer size
    1000,  // One-off with common buffer size
    1024,  // One-off with common buffer size
    4096,  // One-off with common buffer size
    32767, // Overflow signed 16-bit when incremented
    32768, // Overflow signed 16-bit when decremented
    65407, // Overflow signed 8-bit
]);

impl_scalar!(u32 => set_length32, perturb32, replace_magic32 = [
    32768,         // Overflow signed 16-bit
    65535,         // Overflow unsig 16-bit when incremented
    65536,         // Overflow unsig 16 bit
    100_663_045,   // Large positive number (endian-agnostic)
    2_147_483_647, // Overflow signed 32-bit when incremented
    2_147_483_648, // Overflow signed 32-bit when decremented
    4_194_304_250, // Large negative number (endian-agnostic)
    4_294_934_527, // Overflow signed 16-bit
]);

impl_scalar!(u64 => set_length64, perturb64);

fn do_insert_one(rng: &mut RngType, buf: &mut Vec<u8>, r: RangeInclusive<u64>, val: u8) -> bool {
    if buf.len() < *r.end() as usize {
        let pos = rng.gen_range(0..=buf.len());
        buf.insert(pos, val);
        return true;
    }
    false
}

/// Inserts a byte.
fn insert8(
    _ctx: &mut Context,
    rng: &mut RngType,
    buf: &mut Vec<u8>,
    r: RangeInclusive<u64>,
) -> bool {
    let val = rng.gen();
    do_insert_one(rng, buf, r, val)
}

/// Inserts a special byte.
fn insert_special8(
    _ctx: &mut Context,
    rng: &mut RngType,
    buf: &mut Vec<u8>,
    r: RangeInclusive<u64>,
) -> bool {
    let val = *SPECIAL_BYTE.choose(rng).unwrap();
    do_insert_one(rng, buf, r, val)
}

/// Flips a bit.
fn modify1(
    _ctx: &mut Context,
    rng: &mut RngType,
    buf: &mut Vec<u8>,
    _r: RangeInclusive<u64>,
) -> bool {
    let pos = rng.gen_range(0..buf.len());
    buf[pos] ^= 1 << rng.gen_range(0..8);
    true
}

/// Randomly changes a byte.
fn modify8(
    _ctx: &mut Context,
    rng: &mut RngType,
    buf: &mut Vec<u8>,
    _r: RangeInclusive<u64>,
) -> bool {
    let pos = rng.gen_range(0..buf.len());
    buf[pos] ^= 1 + rng.gen_range(0..255);
    true
}

/// Changes a byte to special byte.
fn modify_special8(
    _ctx: &mut Context,
    rng: &mut RngType,
    buf: &mut Vec<u8>,
    _r: RangeInclusive<u64>,
) -> bool {
    let pos = rng.gen_range(0..buf.len());
    buf[pos] = *SPECIAL_BYTE.choose(rng).unwrap();
    true
}

fn print_int(buf: &mut [u8], mut v: u64) {
    for i in buf.iter_mut().rev() {
        *i = (v % 10) as u8 + b'0';
        v /= 10;
    }
}

/// Returns (value, end). Stops until overflow or non-digit.
fn parse_int(buf: &[u8], start: usize) -> (u64, usize) {
    /// Pushes digit `x` into `v`. Returns `None` if overflow.
    fn push_digit(v: u64, x: u8) -> Option<u64> {
        v.checked_mul(10)?.checked_add(x as u64)
    }

    let mut v = 0;
    for (i, x) in buf.iter().enumerate().skip(start) {
        if !x.is_ascii_digit() {
            return (v, i);
        }
        match push_digit(v, x - b'0') {
            Some(pushed) => v = pushed,
            None => return (v, i),
        }
    }
    (v, buf.len())
}

/// Changes an ASCII integer.
fn modify_ascii_int(
    _ctx: &mut Context,
    rng: &mut RngType,
    buf: &mut Vec<u8>,
    _r: RangeInclusive<u64>,
) -> bool {
    fn find_first_digit(buf: &[u8], start: usize) -> Result<usize, ()> {
        let offset = buf[start..]
            .iter()
            .position(|&x| x.is_ascii_digit())
            .ok_or(())?;
        Ok(start + offset)
    }

    let start = if let Ok(start) = find_first_digit(buf, rng.gen_range(0..buf.len())) {
        start
    } else {
        return false;
    };
    let (v, end) = parse_int(buf, start);

    let new = match rng.gen_range(0..5) {
        0 => v.wrapping_add(1),
        1 => v.wrapping_sub(1),
        2 => v.wrapping_div(2),
        3 => v.wrapping_mul(2),
        4 => rng.gen_range(0..v.wrapping_mul(v) | 1),
        _ => unreachable!(),
    };
    print_int(&mut buf[start..end], new);
    true
}

#[cfg(test)]
mod tests {
    use crate::context::Context;
    use rand::{prelude::SmallRng, Rng, SeedableRng};
    use std::ops::RangeInclusive;

    fn rand_buf(rng: &mut SmallRng) -> Vec<u8> {
        let len = rng.gen_range(8..=128);
        (0..=len).map(|_| rng.gen()).collect()
    }

    #[test]
    fn shuffle_empty() {
        let mut ctx = Context::dummy();
        let mut rng = SmallRng::from_entropy();
        let r = RangeInclusive::new(0, 0);
        let mut empty = Vec::new();
        assert!(!super::shuffle(&mut ctx, &mut rng, &mut empty, r.clone()));
    }

    #[test]
    fn erase_empty() {
        let mut ctx = Context::dummy();
        let mut rng = SmallRng::from_entropy();
        let r = RangeInclusive::new(0, 0);
        let mut empty = Vec::new();
        assert!(!super::erase(&mut ctx, &mut rng, &mut empty, r.clone()));
    }

    #[test]
    fn insert_new_content() {
        let mut rng = SmallRng::from_entropy();
        let r = RangeInclusive::new(0, 4);
        let mut buf = Vec::new();
        super::insert_new_content(&mut rng, &mut buf, r, |_, i| *i = 0);
        // insert to end
        assert!(buf.len() >= 2 && buf.len() <= 4);
        // no left space
        assert!(!super::insert_new_content(
            &mut rng,
            &mut buf,
            0..=1,
            |_, i| *i = 0
        ));
        for _ in 0..1024 {
            let len = rng.gen_range(0..=64);
            let max_len = rng.gen_range(len + 2..=len + 64);
            let mut buf = Vec::new();
            buf.resize(len, 0);
            assert!(super::insert_new_content(
                &mut rng,
                &mut buf,
                0..=(max_len as u64),
                |_, i| *i = 0
            ))
        }
    }

    #[test]
    fn all_mutations_not_panic() {
        let mut ctx = Context::dummy();
        let mut rng = SmallRng::from_entropy();
        let r = RangeInclusive::new(0, 256);

        for op in super::BLOB_MUTATE_OPERATIONS {
            for _ in 0..4096 {
                let mut v = rand_buf(&mut rng);
                op(&mut ctx, &mut rng, &mut v, r.clone());
            }
        }
    }
}
