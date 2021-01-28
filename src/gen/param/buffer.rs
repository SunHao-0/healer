use crate::gen::{param::scalar::*, *};
use crate::model::{BufferKind, Dir, TextKind, Value};

use std::collections::VecDeque;
use std::iter::Iterator;

use rustc_hash::FxHashSet;

pub(super) fn gen(
    ctx: &mut GenContext,
    ty: TypeRef,
    dir: Dir, /*For now, just ignore dir.*/
) -> Value {
    let val = match ty.buffer_kind().unwrap() {
        BufferKind::BlobRand => gen_blob(ctx.pool.get(&ty), None),
        BufferKind::BlobRange(min, max) => gen_blob(ctx.pool.get(&ty), Some((*min, *max))),
        BufferKind::Filename { vals, noz } => {
            let fname = gen_fname(
                ctx.pool.get(&ty),
                ctx.generated_str.get(&ty),
                &vals[..],
                *noz,
            );
            ctx.add_str(ty, fname.clone());
            fname
        }
        BufferKind::String { vals, noz } => {
            let new_str = gen_str(
                ctx.pool.get(&ty),
                ctx.generated_str.get(&ty),
                &vals[..],
                *noz,
            );
            ctx.add_str(ty, new_str.clone());
            new_str
        }
        BufferKind::Text(kind) => gen_text(kind),
    };
    Value::new(dir, ty, ValueKind::new_bytes(val))
}

fn gen_fname(
    pool: Option<&VecDeque<Arc<Value>>>,
    generated: Option<&FxHashSet<Box<[u8]>>>,
    vals: &[Box<[u8]>],
    noz: bool,
) -> Box<[u8]> {
    gen_str_like(pool, generated, vals, noz, rand_fname, mutate_fname)
}

const SPECIAL_PATH: [&[u8]; 3] = [b".", b"..", b"~"];

fn rand_fname() -> Vec<u8> {
    let mut rng = thread_rng();
    let mut f = Vec::from(*SPECIAL_PATH.iter().choose(&mut rng).unwrap());
    dec_probability_iter(3, || {
        let new_file = rand_fname_one();
        f.push(b'/');
        f.extend(new_file.iter());
    });
    f
}

fn rand_fname_one() -> Box<[u8]> {
    let mut new_file = rand_blob_range((4, 8));
    filter_noz(&mut new_file);
    new_file
}

fn filter_noz(s: &mut [u8]) {
    for n in s.iter_mut() {
        if *n == b'\0' {
            *n = 0xFF;
        }
    }
}

pub fn mutate_fname(fname: &mut Vec<u8>) {
    // TODO add more mutate operation.
    if *fname.last().unwrap() == b'\0' {
        fname.pop();
    }
    fname.push(b'/');
    fname.extend(rand_fname_one().iter());
}

fn gen_str(
    pool: Option<&VecDeque<Arc<Value>>>,
    generated: Option<&FxHashSet<Box<[u8]>>>,
    vals: &[Box<[u8]>],
    noz: bool,
) -> Box<[u8]> {
    gen_str_like(pool, generated, vals, noz, rand_str, mutate_str)
}

const PUNCT: [u8; 23] = [
    b'!', b'@', b'#', b'$', b'%', b'^', b'&', b'*', b'(', b')', b'-', b'+', b'\\', b'/', b':',
    b'.', b',', b'-', b'\'', b'[', b']', b'{', b'}',
];

fn rand_str() -> Vec<u8> {
    let mut ret = Vec::new();
    let mut rng = thread_rng();
    dec_probability_iter(8, || {
        let b = if rng.gen_ratio(1, 10) {
            PUNCT.iter().copied().choose(&mut rng).unwrap()
        } else {
            rng.gen::<u8>()
        };
        ret.push(b);
    });
    filter_noz(&mut ret);
    ret
}

pub fn mutate_str(val: &mut Vec<u8>) {
    mutate_blob(val, None, gen_range())
}

#[allow(clippy::new_without_default)]
fn gen_str_like<G, M>(
    pool: Option<&VecDeque<Arc<Value>>>,
    generated: Option<&FxHashSet<Box<[u8]>>>,
    vals: &[Box<[u8]>],
    noz: bool,
    gen_new: G,
    mutate: M,
) -> Box<[u8]>
where
    G: FnOnce() -> Vec<u8>,
    M: FnOnce(&mut Vec<u8>),
{
    let mut val = try_reuse(pool, generated, vals);
    if val.is_none() || val.as_ref().unwrap().is_empty() {
        val = Some(gen_new());
    } else if thread_rng().gen_ratio(1, 100) {
        mutate(val.as_mut().unwrap());
    }
    let mut val = val.unwrap();
    for v in val.iter_mut() {
        if *v == b'\0' {
            *v = 0xFF;
        }
    }
    if !noz {
        val.push(b'\0');
    }
    val.into_boxed_slice()
}

#[allow(clippy::unnecessary_unwrap)]
fn try_reuse(
    pool: Option<&VecDeque<Arc<Value>>>,
    generated: Option<&FxHashSet<Box<[u8]>>>,
    vals: &[Box<[u8]>],
) -> Option<Vec<u8>> {
    let mut val = None;
    let mut rng = thread_rng();
    if !vals.is_empty() && rng.gen_ratio(8, 10) {
        val = vals
            .iter()
            .filter(|val| !val.is_empty())
            .choose(&mut rng)
            .map(|x| Vec::from(&**x))
    } else if generated.is_some() && rng.gen_ratio(8, 10) {
        val = generated
            .unwrap()
            .iter()
            .choose(&mut rng)
            .map(|x| Vec::from(&**x))
    } else if pool.is_some() && rng.gen_ratio(8, 10) {
        val = pool
            .unwrap()
            .iter()
            .choose(&mut rng)
            .map(|x| Vec::from(x.bytes_val().unwrap()))
    };
    val
}

fn gen_text(_kind: &TextKind) -> Box<[u8]> {
    rand_blob_range(gen_range())
}

fn gen_blob(pool: Option<&VecDeque<Arc<Value>>>, range: Option<(u64, u64)>) -> Box<[u8]> {
    let range = range
        .map(|(min, max)| (min as usize, max as usize))
        .unwrap_or_else(gen_range);
    let mut rng = thread_rng();
    if rng.gen_ratio(8, 10) {
        mutate_exist_blob(pool, range)
    } else {
        rand_blob_range(range)
    }
}

fn gen_range() -> (usize, usize) {
    let mut rng = thread_rng();
    let min_range = 4..=8;
    let max_range = 16..=128;
    let min = min_range.choose(&mut rng).unwrap();
    let max = max_range.choose(&mut rng).unwrap();
    (min, max)
}

fn mutate_exist_blob(pool: Option<&VecDeque<Arc<Value>>>, (min, max): (usize, usize)) -> Box<[u8]> {
    match pool {
        Some(ref pool) if !pool.is_empty() => {
            let mut src = pool
                .iter()
                .choose(&mut thread_rng())
                .map(|v| Vec::from(v.bytes_val().unwrap()))
                .unwrap();
            if src.is_empty() {
                return rand_blob_range((min, max));
            }
            mutate_blob(&mut src, Some(pool), (min, max));
            src.into_boxed_slice()
        }
        _ => rand_blob_range((min, max)),
    }
}

pub fn mutate_blob(
    src: &mut Vec<u8>,
    pool: Option<&VecDeque<Arc<Value>>>,
    (min, max): (usize, usize),
) {
    let mut delta = 0.8;
    let mut rng = thread_rng();
    dec_probability_iter(4, || {
        let operator = MUTATE_OPERATOR.iter().choose(&mut rng).unwrap();
        operator(src);
        delta *= 0.5;
    });

    if let Some(pool) = pool {
        if pool.len() > 2 && src.len() < max && rng.gen_bool(delta) {
            let oth = pool
                .iter()
                .choose(&mut rng)
                .map(|v| v.bytes_val().unwrap())
                .unwrap();
            splice(src, oth);
        }
    }

    if src.len() < min {
        let pad = rng.gen_range((min - src.len())..(max - src.len() + 1));
        src.extend((0..pad).map(|_| gen_integer(8, None, 1) as u8));
    } else if src.len() > max {
        src.truncate(max);
    }
}

const MUTATE_OPERATOR: [fn(&mut Vec<u8>); 3] = [insert_content, flip_bits, replace_content];

fn insert_content(dst: &mut Vec<u8>) {
    let new_blob = rand_blob_range((4, 32));
    let insert_point = thread_rng().gen_range(0..dst.len());
    do_insert(dst, &new_blob, insert_point);
}

fn do_insert(dst: &mut Vec<u8>, src: &[u8], insert_point: usize) {
    assert!(dst.len() > insert_point);
    use std::ptr::copy;
    let remaining = dst.len() - insert_point;
    dst.reserve(src.len());
    unsafe { dst.set_len(dst.len() + src.len()) };

    let start_ptr = &mut dst[insert_point] as *mut u8;
    let end_ptr = &mut dst[insert_point + src.len()] as *mut u8;
    unsafe {
        copy(start_ptr, end_ptr, remaining);
        copy(&src[0] as *const _, start_ptr, src.len());
    }
}

fn flip_bits(src: &mut Vec<u8>) {
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
    let insert_point = thread_rng().gen_range(0..dst.len());
    do_insert(dst, src, insert_point);
}

fn rand_sub_slice(src: &mut [u8]) -> &mut [u8] {
    assert!(!src.is_empty());
    let mut rng = thread_rng();
    let min = std::cmp::min(2, src.len());
    let len = rng.gen_range(min..(src.len() + 1));
    let end = src.len() - len;
    let sub_start = rng.gen_range(0..(end + 1));
    &mut src[sub_start..sub_start + len]
}

fn rand_blob_range((min, max): (usize, usize)) -> Box<[u8]> {
    let min64 = (min + 7) / 8;
    let max64 = (max + 7) / 8;
    let mut buf = (0..=min64)
        .map(|_| gen_integer(0, None, 1))
        .collect::<Vec<u64>>();
    dec_probability_iter(max64 - min64, || buf.push(gen_integer(64, None, 1)));
    map_vec_64_8(buf).into_boxed_slice()
}

fn dec_probability_iter<F: FnMut()>(t: usize, mut f: F) {
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
    a.shrink_to_fit();
    let len = a.capacity() * 8;
    let ptr = a.as_ptr() as *mut _;
    std::mem::forget(a);
    unsafe { Vec::from_raw_parts(ptr, len, len) }
}
