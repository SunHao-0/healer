//! Generate value for `blob`, `string`, `filename` type.
#![allow(clippy::unit_arg)]
use rand::prelude::*;
use std::ops::RangeInclusive;

use crate::{
    context::Context,
    gen::choose_weighted,
    ty::{BufferStringType, Dir, Type},
    value::{DataValue, Value},
    RngType,
};

pub fn gen_buffer_blob(_ctx: &mut Context, rng: &mut RngType, ty: &Type, dir: Dir) -> Value {
    let ty = ty.checked_as_buffer_blob();
    let range = ty.range().unwrap_or_else(|| rand_blob_range(rng));
    let len = rng.gen_range(range);
    if dir == Dir::Out {
        DataValue::new_out_data(ty.id(), dir, len).into()
    } else {
        DataValue::new(ty.id(), dir, rand_blob(rng, len as usize)).into()
    }
}

fn rand_blob_range(rng: &mut RngType) -> RangeInclusive<u64> {
    const LENS: [u64; 4] = [64, 128, 256, 4096];
    const WEIGHTS: [u64; 4] = [60, 80, 95, 100];
    let idx = choose_weighted(rng, &WEIGHTS);
    0..=LENS[idx]
}

fn rand_blob(rng: &mut RngType, len: usize) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::with_capacity(len);
    // SAFETY: sizeof `buf` equals to `len`.
    unsafe { buf.set_len(len) };
    let (prefix, shorts, suffix) = unsafe { buf.align_to_mut::<u64>() };
    prefix.fill_with(|| rng.gen());
    shorts.fill_with(|| rng.gen());
    suffix.fill_with(|| rng.gen());
    buf
}

pub fn gen_buffer_string(ctx: &mut Context, rng: &mut RngType, ty: &Type, dir: Dir) -> Value {
    let ty = ty.checked_as_buffer_string();
    if dir == Dir::Out {
        let len = if let Some(val) = ty.vals().choose(rng) {
            val.len() as u64
        } else {
            let r = rand_blob_range(rng);
            rng.gen_range(r)
        };
        DataValue::new_out_data(ty.id(), dir, len).into()
    } else {
        let val = rand_buffer_string(ctx, rng, ty);
        DataValue::new(ty.id(), dir, val).into()
    }
}

fn rand_buffer_string(ctx: &mut Context, rng: &mut RngType, ty: &BufferStringType) -> Vec<u8> {
    if let Some(val) = ty.vals().choose(rng) {
        return Vec::from(&val[..]);
    }

    if !ctx.strs().is_empty() && rng.gen() {
        return ctx.strs().choose(rng).cloned().unwrap();
    }

    let mut val = gen_rand_string(rng);
    if !ty.noz() {
        if val.is_empty() {
            val.push(0);
        } else {
            *val.last_mut().unwrap() = 0;
        }
    }
    if val.len() > 3 {
        ctx.record_str(val.clone());
    }

    val
}

fn gen_rand_string(rng: &mut RngType) -> Vec<u8> {
    const PUNCT: [u8; 23] = [
        b'!', b'@', b'#', b'$', b'%', b'^', b'&', b'*', b'(', b')', b'-', b'+', b'\\', b'/', b':',
        b'.', b',', b'-', b'\'', b'[', b']', b'{', b'}',
    ];

    let r = rand_blob_range(rng);
    let len = rng.gen_range(r) as usize;
    let mut buf = Vec::with_capacity(len);
    unsafe { buf.set_len(len) };
    for val in buf.iter_mut() {
        if rng.gen() {
            *val = PUNCT.choose(rng).copied().unwrap();
        } else {
            *val = rng.gen_range(0..=255);
        }
    }
    buf
}

pub fn gen_buffer_filename(ctx: &mut Context, rng: &mut RngType, ty: &Type, dir: Dir) -> Value {
    // let ty = ty.checked_as_buffer_filename();
    if dir == Dir::Out {
        let len = if !ty.varlen() {
            ty.size()
        } else {
            rand_filename_len(rng)
        };
        DataValue::new_out_data(ty.id(), dir, len).into()
    } else {
        let noz = if let Some(ty) = ty.as_buffer_string() {
            ty.noz()
        } else {
            let ty = ty.checked_as_buffer_filename();
            ty.noz()
        };
        let val = rand_filename(ctx, rng, ty, noz);
        DataValue::new(ty.id(), dir, val).into()
    }
}
pub const UNIX_PATH_MAX: u64 = 108;
pub const PATH_MAX: u64 = 4096;

#[inline]
fn rand_filename_len(rng: &mut RngType) -> u64 {
    match rng.gen_range(0..=2) {
        0 => rng.gen_range(0..100),
        1 => UNIX_PATH_MAX,
        _ => PATH_MAX,
    }
}

pub(crate) fn rand_filename(ctx: &mut Context, rng: &mut RngType, ty: &Type, noz: bool) -> Vec<u8> {
    const GENERATORS: [fn(&mut Context, &mut RngType) -> Vec<u8>; 3] =
        [rand_reusing, gen_rand_filename, rand_speical_files];
    const WEIGHTS: [u64; 3] = [80, 99, 100];

    let idx = choose_weighted(rng, &WEIGHTS);
    let mut val = GENERATORS[idx](ctx, rng);
    // TODO clean filename
    if !ty.varlen() {
        let sz = ty.size() as usize;
        if val.len() != sz {
            val.resize(sz, 0);
        }
    } else if !noz {
        val.push(0);
    }
    val.shrink_to_fit();
    val
}

fn rand_speical_files(_ctx: &mut Context, rng: &mut RngType) -> Vec<u8> {
    const SPEICAL_FILES: [&[u8]; 2] = [b"", b"."];
    SPEICAL_FILES.choose(rng).map(|v| Vec::from(*v)).unwrap()
}

fn rand_reusing(ctx: &mut Context, rng: &mut RngType) -> Vec<u8> {
    if let Some(f) = ctx.filenames().choose(rng) {
        f.clone()
    } else {
        gen_rand_filename_inner(ctx, rng, None)
    }
}

fn gen_rand_filename(ctx: &mut Context, rng: &mut RngType) -> Vec<u8> {
    let base = if !ctx.filenames().is_empty() && rng.gen() {
        let mut base = ctx.filenames().choose(rng).cloned().unwrap();
        if *base.last().unwrap() == 0 {
            base.pop();
        }
        Some(base)
    } else {
        None
    };
    gen_rand_filename_inner(ctx, rng, base)
}

fn gen_rand_filename_inner(ctx: &mut Context, rng: &mut RngType, base: Option<Vec<u8>>) -> Vec<u8> {
    let mut fname = vec![b'.'];
    if let Some(base) = base {
        if !base.is_empty() {
            fname = base;
        }
    }
    if fname.len() != 1 && fname[0] != b'.' && rng.gen_ratio(1, 10) {
        fname.extend_from_slice(b"/..");
    }
    let mut i = 0;
    loop {
        fname.extend_from_slice(b"/file");
        fname.extend_from_slice(i.to_string().as_bytes());
        if ctx.record_filename(&fname) {
            return fname;
        }
        i += 1;
    }
}
