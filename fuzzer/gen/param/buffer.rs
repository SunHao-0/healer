use super::{scalar::*, *};
use hlang::ast::{BufferKind, Dir, TextKind, Type, Value};
use rustc_hash::FxHashSet;
use std::iter::Iterator;
use std::rc::Rc;

pub(super) fn gen(ctx: &mut GenContext, ty: Rc<Type>, dir: Dir) -> Value {
    use BufferKind::*;
    let val = match ty.get_buffer_kind().unwrap() {
        Blob => gen_blob(ctx.pool.get(&ty), None),
        BlobRange(min, max) => gen_blob(ctx.pool.get(&ty), Some((*min, *max))),
        Filename { vals, noz } => {
            let fname = gen_fname(ctx.pool.get(&ty), ctx.generated_buf.get(&ty), vals, noz);
            ctx.add_str(Rc::clone(&ty), fname.clone());
            fname
        }
        BufferKind::String { vals, noz } => {
            let new_str = gen_str(ctx.pool.get(&ty), ctx.generated_buf.get(&ty), vals, noz);
            ctx.add_str(Rc::clone(&ty), new_str.clone());
            new_str
        }
        Text(kind) => gen_text(kind),
    };
    Value::new_bytes(dir, ty, val)
}

fn gen_fname(
    pool: &Option<FxHashSet<Value>>,
    generated: &Option<FxHashSet<Box<str>>>,
    vals: &[Box<[u8]>],
    noz: bool,
) -> Box<[u8]> {
    todo!()
}

fn gen_str(
    pool: &Option<FxHashSet<Value>>,
    generated: &Option<FxHashSet<Box<str>>>,
    vals: &[Box<[u8]>],
    noz: bool,
) -> Box<[u8]> {
    todo!()
}

fn gen_text(kind: &TextKind) -> Box<u8> {
    todo!()
}

fn gen_blob(pool: &Option<FxHashSet<Value>>, range: Option<(u64, u64)>) -> Box<[u8]> {
    let range = range.unwrap_or_else(gen_range);
    let mut rng = thread_rng();
    if rng.gen::<f32>() < 0.8 {
        mutate_exist_blob(pool, range)
    } else {
        rand_blob_range(range)
    }
}

fn gen_range() -> (u64, u64) {
    let mut rng = thread_rng();
    let min_range = 4..=8;
    let max_range = 16..=128;
    let min = min_range.choose(&mut rng).unwrap();
    let max = min_range.choose(&mut rng).unwrap();
    rand_blob_range(min, max)
}

fn mutate_exist_blob(pool: &Option<FxHashSet<Value>>, (min, max): (u64, u64)) -> Box<[u8]> {
    match pool {
        Some(ref pool) if !pool.is_empty() => {
            let mut rng = thread_rng();
            let select = || {
                pool.iter()
                    .choose(&mut rng)
                    .map(|v| Vec::from(v.get_bytes_val()))
                    .unwrap()
            };
            let mut src = select();
            let mut delta = 0.8;
            dec_probability_iter(4, || {
                let operator = MUTATE_OPERATOR.iter().choose(&mut rng).unwrap();
                operator(&mut src);
                delta *= 0.5;
            });
            if pool.len() > 2 && src.len() < max && rng.gen::<f32>() < delta {
                let oth = select();
                splice(&mut src, &oth);
            }
            if src.len() < min {
                src.resize_with(rng.gen_range(min - src.len(), max - src.len()), || {
                    gen_integer(8, None, 1) as u8
                });
            } else if src.len() > max {
                src.resize(max);
            }

            src.into_boxed_slice()
        }
        _ => rand_blob_range((min, max)),
    }
}

const MUTATE_OPERATOR: [fn(&mut Vec<u8>); 3] = [insert_content, bit_flip, replace_content];

fn insert_content(dst: &mut Vec<u8>) {
    let new_blob = rand_blob_range((4, 32));
    let insert_point = thread_rng().gen_range(0, dst.len());
    do_insert(dst, &new_blob, insert_point);
}

fn do_insert(dst: &Vec<u8>, src: &[u8], insert_point: usize) {
    assert!(dst.len() > insert_point);
    use std::ptr::copy;
    let remaining = dst.len() - insert_point;
    dst.reserve(src.len());
    unsafe { dst.set_len(dst.len() + src.len()) };

    let start_ptr = &mut dst[insert_point] as *mut u8;
    let end_ptr = &mut dst[insert_point + src.len()] as *mut u8;
    copy(start_ptr, end_ptr, remaining);
    copy(&src[0] as *const _, start_ptr, src.len());
}

fn bit_flip(src: &mut Vec<u8>) {
    // TODO operate on 8 bytes one time.
    let sub = rand_sub_slice(src);
    for n in sub {
        *n ^= 0xFF;
    }
}

fn replace_content(src: &mut Vec<u8>) {
    let sub = rand_sub_slice(src);
    for n in sub {
        // TODO operate on 8 bytes one time.
        *n = gen_integer(64, None, 1) as u8;
    }
}

fn splice(dst: &mut Vec<u8>, src: &[u8]) {
    let insert_point = thread_rng().gen_range(0, dst.len());
    do_insert(dst, src, insert_point);
}

fn rand_sub_slice(src: &mut [u8]) -> &mut [u8] {
    assert!(!src.empty());
    let mut rng = thread_rng();
    let min = std::cmp::min(2, src.len());
    let len: usize = rng.gen_range(min, src.len() + 1);
    let end = src.len() - len;
    let sub_start = rng.gen_range(0, end + 1);
    &mut src[sub_start..sub_start + len]
}

fn rand_blob_range((min, max): (u64, u64)) -> Box<[u8]> {
    let min64 = (min + 7) / 8;
    let max64 = (max + 7) / 8;
    let mut buf = (0..=min64)
        .map(|_| gen_integer(0, None, 1))
        .collect::<Vec<u64>>();
    dec_probability_iter(max64, || buf.push(gen_integer(0, None, 1)));
    map_vec_64_8(buf).into_boxed_slice()
}

fn dec_probability_iter<F: FnMut() -> ()>(t: u64, f: F) {
    let mut i = 0;
    let mut delta = 0.8;
    let mut rng = thread_rng();

    while i != t && rng.gen::<f32>() < delta {
        f();
        i += 1;
        delta *= 0.5;
    }
}

fn map_vec_64_8(mut a: Vec<u64>) -> Vec<u8> {
    a.shrink_to_fix();
    let len = a.capacity() * 8;
    let ptr: *const u8 = a.as_ptr() as *const u8;
    std::mem::forget(a);
    unsafe { Vec::from_raw_parts(ptr, len, len) }
}
