//! Prog generation.
use self::{
    buffer::{gen_buffer_blob, gen_buffer_filename, gen_buffer_string},
    group::{gen_array, gen_struct, gen_union},
    int::{
        gen_const, gen_csum, gen_flags, gen_int, gen_len, gen_proc, len_calculated,
        need_calculate_len,
    },
    ptr::{gen_ptr, gen_vma},
    res::gen_res,
};
use crate::{
    context::Context,
    len::calculate_len,
    prog::{CallBuilder, Prog},
    relation::Relation,
    select::select,
    syscall::SyscallId,
    target::Target,
    ty::{Dir, Type, TypeKind},
    value::{ResValue, Value},
    verbose, RngType,
};
use rand::prelude::*;
use std::{
    cell::{Cell, RefCell},
    ops::Range,
};

pub mod buffer;
pub mod group;
pub mod int;
pub mod ptr;
pub mod res;

const MIN_PROG_LEN: usize = 16;
const MAX_PROG_LEN: usize = 25;

thread_local! {
    static NEXT_PROG_LEN: Cell<usize> = Cell::new(MIN_PROG_LEN);
    static PROG_LEN_RANGE: Cell<(usize, usize)> = Cell::new((MIN_PROG_LEN, MAX_PROG_LEN));
}

/// Set prog length range
#[inline]
pub fn set_prog_len_range(r: Range<usize>) {
    PROG_LEN_RANGE.with(|v| v.set((r.start, r.end)))
}

/// Get current prog length range
#[inline]
pub fn prog_len_range() -> Range<usize> {
    PROG_LEN_RANGE.with(|r| {
        let v = r.get();
        Range {
            start: v.0,
            end: v.1,
        }
    })
}

fn next_prog_len() -> usize {
    NEXT_PROG_LEN.with(|next_len| {
        let len = next_len.get();
        let r = prog_len_range();
        let mut new_len = len + 1;
        if new_len >= r.end {
            new_len = MIN_PROG_LEN
        };
        next_len.set(new_len);
        len
    })
}

/// Generate prog based on `target` and `relation`.
pub fn gen_prog(target: &Target, relation: &Relation, rng: &mut RngType) -> Prog {
    let mut ctx = Context::new(target, relation);
    let len = next_prog_len();
    if verbose() {
        log::info!("prog len: {}", len);
    }
    while ctx.calls().len() < len {
        gen_call(&mut ctx, rng);
    }
    if verbose() {
        log::info!("Context:\n{}", ctx);
    }
    ctx.to_prog()
}

/// Add a syscall to `context`.
#[inline]
pub fn gen_call(ctx: &mut Context, rng: &mut RngType) {
    let sid = select(ctx, rng);
    gen_one_call(ctx, rng, sid)
}

thread_local! {
    static CALLS_STACK: RefCell<Vec<CallBuilder>> = RefCell::new(Vec::with_capacity(4));
}

#[inline]
fn push_builder(sid: SyscallId) {
    CALLS_STACK.with(|calls| calls.borrow_mut().push(CallBuilder::new(sid)))
}

#[inline]
fn current_builder<F>(mut f: F)
where
    F: FnMut(&mut CallBuilder),
{
    CALLS_STACK.with(|calls| f(calls.borrow_mut().last_mut().unwrap()))
}

#[inline]
fn pop_builder() -> CallBuilder {
    CALLS_STACK.with(|calls| calls.borrow_mut().pop().unwrap())
}

/// Generate syscall `sid` to `context`.
pub fn gen_one_call(ctx: &mut Context, rng: &mut RngType, sid: SyscallId) {
    push_builder(sid);
    if verbose() {
        log::info!("generating: {}", ctx.target().syscall_of(sid));
        CALLS_STACK.with(|stack| {
            log::info!("current stack: {:?}", stack);
        })
    }
    let syscall = ctx.target().syscall_of(sid);
    let mut args = Vec::with_capacity(syscall.params().len());
    for param in syscall.params() {
        args.push(gen_ty_value(
            ctx,
            rng,
            param.ty(),
            param.dir().unwrap_or(Dir::In),
        ));
    }
    if need_calculate_len() {
        calculate_len(ctx.target(), syscall, &mut args);
        len_calculated();
    }
    let ret = syscall.ret().map(|ty| {
        assert!(!ty.optional());
        gen_ty_value(ctx, rng, ty, Dir::Out)
    });

    let mut builder = pop_builder();
    builder.args(args).ret(ret);
    ctx.append_call(builder.build());
}

pub type Generator = fn(&mut Context, &mut RngType, &Type, Dir) -> Value;
pub const GENERATOR: [Generator; 15] = [
    gen_res,
    gen_const,
    gen_int,
    gen_flags,
    gen_len,
    gen_proc,
    gen_csum,
    gen_vma,
    gen_buffer_blob,
    gen_buffer_string,
    gen_buffer_filename,
    gen_array,
    gen_ptr,
    gen_struct,
    gen_union,
];

pub fn gen_ty_value(ctx: &mut Context, rng: &mut RngType, ty: &Type, dir: Dir) -> Value {
    use TypeKind::*;

    if dir == Dir::Out && matches!(ty.kind(), Const | Int | Flags | Proc | Vma) {
        ty.default_value(dir)
    } else if ty.optional() && rng.gen_ratio(1, 5) {
        if let Some(ty) = ty.as_res() {
            let v = ty.special_vals().choose(rng).unwrap();
            return ResValue::new_null(ty.id(), dir, *v).into();
        }
        ty.default_value(dir)
    } else {
        GENERATOR[ty.kind() as usize](ctx, rng, ty, dir)
    }
}

/// Return chosen index based on `weights`.
///
/// Weight is accumulated value. For example, [10, 35, 50] means each item has
/// 10%, 25%, 15% to be chosen.
pub(crate) fn choose_weighted(rng: &mut RngType, weights: &[u64]) -> usize {
    let max = weights.last().unwrap();
    let n = rng.gen_range(0..*max);
    match weights.binary_search(&n) {
        Ok(idx) => idx + 1,
        Err(idx) => idx,
    }
}

#[cfg(test)]
mod tests {
    use rand::{prelude::SmallRng, SeedableRng};

    #[test]
    fn next_prog_len() {
        assert_eq!(super::next_prog_len(), super::MIN_PROG_LEN);
        assert_eq!(super::next_prog_len(), super::MIN_PROG_LEN + 1);
        while super::next_prog_len() != super::MAX_PROG_LEN - 1 {}
        assert_eq!(super::next_prog_len(), super::MIN_PROG_LEN);
    }

    #[test]
    fn choose_weighted() {
        let mut rng = SmallRng::from_entropy();
        let weight = [100];
        assert_eq!(super::choose_weighted(&mut rng, &weight), 0);
        let weights = [10, 20, 100];
        for _ in 0..10 {
            let idx = super::choose_weighted(&mut rng, &weight);
            assert!(idx < weights.len());
        }
    }
}
