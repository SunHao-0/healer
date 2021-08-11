use crate::gen::{param::scalar::*, *};

use std::collections::VecDeque;
use std::iter::Iterator;

pub(super) fn gen(ctx: &mut ProgContext, ty: TypeRef, dir: Dir) -> Value {
    match ty.buffer_kind().unwrap() {
        BufferKind::BlobRand => gen_blob(ty, dir, None),
        BufferKind::BlobRange(min, max) => gen_blob(ty, dir, Some((*min, *max))),
        BufferKind::Filename { vals, noz } => {
            let fname = gen_fname(ctx.strs.get(&ty), &vals[..], *noz);
            ctx.add_str(ty, fname.clone());
            Value::new(dir, ty, ValueKind::new_in_bytes(fname))
        }
        BufferKind::String { vals, noz } => {
            let new_str = gen_str(ctx.strs.get(&ty), &vals[..], *noz);
            ctx.add_str(ty, new_str.clone());
            Value::new(dir, ty, ValueKind::new_in_bytes(new_str))
        }
        BufferKind::Text(kind) => gen_text(ty, dir, kind),
        BufferKind::BufferGlob => {
            let fname = gen_fname(ctx.strs.get(&ty), &Vec::new()[..], random());
            ctx.add_str(ty, fname.clone());
            Value::new(dir, ty, ValueKind::new_in_bytes(fname))
        }
    }
}

fn gen_fname(generated: Option<&Vec<Vec<u8>>>, vals: &[Vec<u8>], noz: bool) -> Vec<u8> {
    gen_str_like(generated, vals, noz, rand_fname, mutate_fname)
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

fn rand_fname_one() -> Vec<u8> {
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

fn gen_str(generated: Option<&Vec<Vec<u8>>>, vals: &[Vec<u8>], noz: bool) -> Vec<u8> {
    gen_str_like(generated, vals, noz, rand_str, mutate_str)
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

fn gen_str_like<G, M>(
    generated: Option<&Vec<Vec<u8>>>,
    vals: &[Vec<u8>],
    noz: bool,
    gen_new: G,
    mutate: M,
) -> Vec<u8>
where
    G: FnOnce() -> Vec<u8>,
    M: FnOnce(&mut Vec<u8>),
{
    let mut val = try_reuse(generated, vals);

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

    val
}

#[allow(clippy::unnecessary_unwrap)]
fn try_reuse(generated: Option<&Vec<Vec<u8>>>, vals: &[Vec<u8>]) -> Option<Vec<u8>> {
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
    };
    val
}

fn gen_text(ty: TypeRef, dir: Dir, _kind: &TextKind) -> Value {
    if dir == Dir::Out {
        let len = thread_rng().gen_range(8..100);
        Value::new(dir, ty, ValueKind::new_out_bytes(len))
    } else {
        let v = rand_blob_range(gen_range());
        Value::new(dir, ty, ValueKind::new_in_bytes(v))
    }
}

fn gen_blob(ty: TypeRef, dir: Dir, range: Option<(u64, u64)>) -> Value {
    let mut r = None;

    if let Some((min, max)) = range {
        if min <= max {
            if min == max {
                if min != 0 {
                    r = Some((0, min as usize));
                }
            } else {
                r = Some((min as usize, max as usize));
            }
        }
    }

    if r.is_none() {
        r = Some(gen_range());
    }

    if dir == Dir::Out {
        let range = r.unwrap();
        let len = thread_rng().gen_range(range.0..range.1);
        Value::new(dir, ty, ValueKind::new_out_bytes(len as u64))
    } else {
        let v = rand_blob_range(r.unwrap());
        Value::new(dir, ty, ValueKind::new_in_bytes(v))
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

fn rand_blob_range((min, max): (usize, usize)) -> Vec<u8> {
    let min64 = (min + 7) / 8;
    let max64 = (max + 7) / 8;
    let mut buf = (0..=min64)
        .map(|_| gen_integer(0, None, 1))
        .collect::<Vec<u64>>();
    dec_probability_iter(max64 - min64, || buf.push(gen_integer(64, None, 1)));
    map_vec_64_8(buf)
}

fn dec_probability_iter<F: FnMut()>(t: usize, mut f: F) {
    let mut i = 0;
    let mut rng = thread_rng();

    while i != t && rng.gen_ratio(9, 10) {
        f();
        i += 1;
    }
}

fn map_vec_64_8(mut a: Vec<u64>) -> Vec<u8> {
    a.shrink_to_fit();
    let len = a.capacity() * 8;
    let ptr = a.as_ptr() as *mut _;
    std::mem::forget(a);
    unsafe { Vec::from_raw_parts(ptr, len, len) }
}
