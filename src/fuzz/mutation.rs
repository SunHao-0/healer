use crate::{
    exec::ExecOpt,
    gen::param::{
        buffer,
        scalar::{gen_flag, gen_integer, MAGIC64},
    },
    model::{BufferKind, IntFmt, Prog, TypeKind, Value, ValueKind},
};

use rand::prelude::*;

pub fn mutate_args(p: &Prog) -> Prog {
    let mut rng = thread_rng();
    let mut p_new = p.clone();
    let mut n = 1;
    if p.calls.len() > 1 {
        n = rng.gen_range(1..p.calls.len());
    };

    let idx = (0..p.calls.len()).collect::<Vec<usize>>();
    let idx = idx
        .choose_multiple_weighted(&mut rng, n, |i| p_new.calls[*i].val_cnt as i32)
        .unwrap();

    for i in idx {
        let c = &mut p_new.calls[*i];
        for val in c.args.iter_mut() {
            mutate_arg(val)
        }
    }
    p_new
}

fn mutate_arg(val: &mut Value) {
    match &mut val.kind {
        ValueKind::Scalar(_) => mutate_scalar(val),
        ValueKind::Ptr { pointee, .. } => {
            if let Some(ref mut pointee) = pointee {
                mutate_arg(pointee)
            }
        }
        ValueKind::Bytes(_) => mutate_buffer(val),
        ValueKind::Group(ref mut vals) => {
            for v in vals.iter_mut() {
                mutate_arg(v)
            }
        }
        ValueKind::Union { ref mut val, .. } => mutate_arg(val),
        ValueKind::Res(_) | ValueKind::Vma { .. } => {}
    }
}

fn mutate_scalar(val: &mut Value) {
    let (val_old, _) = val.scalar_val();
    let val_new = match &val.ty.kind {
        TypeKind::Len { .. } | TypeKind::Const { .. } | TypeKind::Csum { .. } => val_old,
        TypeKind::Int {
            int_fmt,
            range,
            align,
        } => mutate_int(val_old, int_fmt, *range, *align),
        TypeKind::Flags {
            vals,
            bitmask,
            int_fmt,
        } => mutate_flag(val_old, vals, *bitmask, int_fmt),
        TypeKind::Proc { per_proc, .. } => thread_rng().gen_range(0..*per_proc),
        _ => unreachable!(),
    };
    *val = Value::new(val.dir, val.ty, ValueKind::new_scalar(val_new));
}

fn mutate_int(val_old: u64, fmt: &IntFmt, range: Option<(u64, u64)>, align: u64) -> u64 {
    let mut mutated = false;
    let mut rng = thread_rng();
    let mut val_new = val_old;
    if rng.gen_ratio(1, 10) {
        val_new = MAGIC64.choose(&mut rng).unwrap() + val_old & 1;
        if val_new != val_old {
            mutated = true;
        }
    }
    if rng.gen_ratio(3, 10) {
        if rng.gen() {
            val_new -= rng.gen_range(1..=32);
        } else {
            val_new += rng.gen_range(1..=32);
        }
    }
    if !mutated || rng.gen_ratio(1, 100) {
        val_new = gen_integer(fmt.bitfield_len, range, align);
    }

    val_new
}

fn mutate_flag(val_old: u64, vals: &[u64], bitmask: bool, fmt: &IntFmt) -> u64 {
    let mut rng = thread_rng();
    if !bitmask {
        if rng.gen_ratio(4, 10) {
            mutate_int(val_old, fmt, None, 0)
        } else {
            let mut val_new = *vals.choose(&mut rng).unwrap();
            if rng.gen_ratio(1, 100) {
                if rng.gen() {
                    val_new -= rng.gen_range(1..=32);
                } else {
                    val_new += rng.gen_range(1..=32);
                }
            }
            val_new
        }
    } else {
        let mut val_new = val_old;
        if rng.gen_ratio(6, 10) {
            let n = rng.gen_range(0..vals.len());
            for flag in vals.choose_multiple(&mut rng, n) {
                if rng.gen() {
                    val_new |= flag;
                } else {
                    val_new ^= flag;
                }
            }
        }
        if val_new == val_old || rng.gen_ratio(5, 100) {
            val_new = bit_flip(val_new);
        }
        if val_new == val_old {
            val_new = gen_flag(None, vals, bitmask)
        }
        val_new
    }
}

fn bit_flip(val: u64) -> u64 {
    let mut val = val.to_le_bytes();
    let mut rng = thread_rng();
    let n = rng.gen_range(1..=8);
    for bit in (0..64).choose_multiple(&mut rng, n) {
        val[bit / 8] ^= 128 >> (bit & 7);
    }
    u64::from_le_bytes(val)
}

fn mutate_buffer(val: &mut Value) {
    let mut rng = thread_rng();
    let val_inner = val.bytes_val().unwrap();
    let len_old = val_inner.len();
    let mut buf = Vec::from(val_inner);
    let kind = val.ty.buffer_kind().unwrap();

    // Mutation blob has high cost, but low gaining, so keep the mutation frequency low.
    match kind {
        BufferKind::String { vals, noz } => {
            if !vals.is_empty() && rng.gen_ratio(1, 10) {
                buf = Vec::from(&**vals.choose(&mut rng).unwrap());
            }
            buffer::mutate_blob(&mut buf, None, (len_old, len_old));
            if !noz {
                *buf.last_mut().unwrap() = b'\0';
            }
        }
        BufferKind::BlobRand | BufferKind::BlobRange { .. } | BufferKind::Text { .. } => {
            if rng.gen_ratio(1, 100) {
                // Mutation blob has high cost, but low gaining, so keep the mutation frequency low.
                buffer::mutate_blob(&mut buf, None, (len_old, len_old));
            }
        }
        BufferKind::Filename { .. } => (),
    }

    *val = Value::new(val.dir, val.ty, ValueKind::new_bytes(buf));
}

pub fn insert_call(_p: &Prog) -> Prog {
    todo!()
}

pub fn seq_reuse(_p: &Prog) -> Prog {
    todo!()
}

pub fn fault_inject(_p: &Prog) -> ExecOpt {
    todo!()
}
