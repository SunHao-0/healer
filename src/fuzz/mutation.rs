use crate::{
    gen::{
        append,
        context::ProgContext,
        param::{
            buffer,
            scalar::{gen_flag, gen_integer, MAGIC64},
        },
        MAX_LEN,
    },
    model::*,
    targets::Target,
};
use rand::prelude::*;

pub const MUTATE_OP: [fn(&Target, &Prog) -> Prog; 3] = [mutate_args, insert_call, mutate];

pub fn mutate(t: &Target, p: &Prog) -> Prog {
    let mut rng = thread_rng();
    let mut new_p = p.clone();

    let mut changed = false;
    loop {
        if rng.gen_ratio(5, 10) {
            mutate_args_inner(&mut new_p);
        }

        if !changed || rng.gen_ratio(8, 10) {
            insert_inner(t, &mut new_p, false);
            changed = true;
        }

        if new_p.calls.len() >= MAX_LEN || rng.gen_ratio(1, 3) {
            break;
        }
    }

    new_p
}

pub fn mutate_args(_t: &Target, p: &Prog) -> Prog {
    let mut p_new = p.clone();
    mutate_args_inner(&mut p_new);
    p_new
}

fn mutate_args_inner(p: &mut Prog) {
    let mut rng = thread_rng();
    let mut n = 1;
    if p.calls.len() > 1 {
        n = rng.gen_range(1..p.calls.len());
    };

    let idx = (0..p.calls.len()).collect::<Vec<usize>>();
    let idx = idx.choose_multiple(&mut rng, n);

    for i in idx {
        let c = &mut p.calls[*i];
        for val in c.args.iter_mut() {
            mutate_arg(val)
        }
    }
}

fn mutate_arg(val: &mut Value) {
    match &mut val.kind {
        ValueKind::Scalar(_) => mutate_scalar(val),
        ValueKind::Ptr { pointee, .. } => {
            if let Some(ref mut pointee) = pointee {
                mutate_arg(pointee)
            }
        }
        ValueKind::Bytes { .. } => mutate_buffer(val),
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
    let (val_old, _) = val.scalar_val().unwrap();
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
        val_new = MAGIC64.choose(&mut rng).unwrap() + (val_old & 1);
        if val_new != val_old {
            mutated = true;
        }
    }
    if rng.gen_ratio(3, 10) {
        let (val, _) = if rng.gen() {
            val_new.overflowing_sub(rng.gen_range(1..=32))
        } else {
            val_new.overflowing_add(rng.gen_range(1..=32))
        };
        val_new = val;
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
            val_new = gen_flag(vals, bitmask)
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

    match kind {
        BufferKind::String { vals, noz } => {
            if !vals.is_empty() && rng.gen_ratio(1, 10) {
                buf = Vec::from(&**vals.choose(&mut rng).unwrap());
            }
            if !buf.is_empty() {
                buffer::mutate_blob(&mut buf, None, (len_old, len_old));
                if !noz {
                    *buf.last_mut().unwrap() = b'\0';
                }
            }
        }
        &BufferKind::BufferGlob
        | BufferKind::BlobRand
        | BufferKind::BlobRange { .. }
        | BufferKind::Text { .. } => {
            if !buf.is_empty() && rng.gen_ratio(4, 10) {
                // Mutation blob has high cost, but low gaining, so keep the mutation frequency low.
                buffer::mutate_blob(&mut buf, None, (len_old, len_old));
            }
        }
        BufferKind::Filename { .. } => (),
    }

    *val = Value::new(val.dir, val.ty, ValueKind::new_in_bytes(buf));
}

pub fn insert_call(t: &Target, p: &Prog) -> Prog {
    let mut p = p.clone();
    insert_inner(t, &mut p, true);
    p
}

fn insert_inner(t: &Target, p: &mut Prog, mult: bool) {
    let mut rng = thread_rng();
    let mut changed = false;
    while !changed || (mult && rng.gen_ratio(2, 3)) {
        let n = (1..=p.calls.len()).choose(&mut rng).unwrap();
        let left = p.calls.split_off(n);
        let mut ctx = ProgContext::restore(t, &mut p.calls);
        append(&mut ctx, &mut p.calls);
        p.calls.extend(left);
        changed = true;
    }
}

// pub fn seq_reuse(t: &Target, p: &Prog, r: &Relation) -> Prog {
//     let mut comm = (FxHashSet::default(), 0);
//     let mut idx = 0;
//     let mut seq = p.calls.iter().map(|c| c.meta).collect::<Vec<_>>();
//     let mut rng = thread_rng();

//     loop {
//         if let Some(calls) = r.get(seq[idx]) {
//             let calls = calls.clone();
//             if comm.0.is_empty() {
//                 comm = (calls, 1);
//             } else {
//                 let comm_next = calls
//                     .intersection(&comm.0)
//                     .copied()
//                     .collect::<FxHashSet<_>>();
//                 if !comm_next.is_empty() {
//                     comm = (comm_next, comm.1 + 1);
//                 } else if comm.1 <= 1 {
//                     comm = (if rng.gen() { comm_next } else { comm.0 }, 1);
//                 }
//             }
//         }

//         if !comm.0.is_empty()
//             && ((comm.1 <= 1 && rng.gen_ratio(5, 10)) || (comm.1 > 1 && rng.gen_ratio(8, 10)))
//         {
//             let call = if rng.gen_ratio(2, 3) {
//                 comm.0.iter().choose(&mut rng).unwrap()
//             } else {
//                 loop {
//                     let call = t.syscalls.choose(&mut rng).unwrap();
//                     if !call.attr.disable {
//                         break call;
//                     }
//                 }
//             };

//             if idx != seq.len() - 1 {
//                 seq.insert(idx + 1, *call)
//             } else {
//                 seq.push(*call);
//             }
//         }

//         idx += 1;
//         if idx >= seq.len() || seq.len() >= MAX_LEN {
//             break;
//         }
//     }

//     gen_seq(t, pool, &seq)
// }
