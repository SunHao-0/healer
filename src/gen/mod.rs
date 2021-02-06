use crate::fuzz::fuzzer::ValuePool;
use crate::gen::context::GenContext;
use crate::model::*;
use crate::targets::Target;

use std::sync::Arc;

use rand::prelude::*;

// Syscall selection.
pub(crate) mod select;
// Argument value generation.
pub(crate) mod param;
// Call generation.
pub(crate) mod call;
// Length type calculation. It's here because length calculation is inter-params.
pub(crate) mod len;

pub(crate) mod context;

pub(crate) const MAX_LEN: usize = 32;
pub(crate) const MIN_LEN: usize = 4;

/// Gnerate test case based current value pool and test target.
pub fn gen(target: &Target, pool: &ValuePool) -> Prog {
    let mut ctx = GenContext::new(target, pool);
    gen_inner(&mut ctx)
}

pub fn gen_seq(target: &Target, pool: &ValuePool, seq: &[SyscallRef]) -> Prog {
    let mut ctx = GenContext::new(target, pool);
    let mut calls = Vec::new();
    for call in seq {
        calls.push(call::gen(&mut ctx, call));
    }
    Prog::new(calls)
}

fn gen_inner(ctx: &mut GenContext) -> Prog {
    let mut calls: Vec<Call> = Vec::new();
    while !should_stop(calls.len()) {
        append(ctx, &mut calls);
    }
    Prog::new(calls)
}

pub(crate) fn append(ctx: &mut GenContext, calls: &mut Vec<Call>) {
    let next_syscall = select::select_syscall(ctx);
    calls.push(call::gen(ctx, next_syscall));
}

fn should_stop(len: usize) -> bool {
    if len < MIN_LEN {
        // not long enough
        false
    } else if len < MAX_LEN {
        // we can continue, we can alse stop, so we use rand.
        let delta = 0.8 * (len as f64 / MAX_LEN as f64);
        thread_rng().gen_bool(delta)
    } else {
        true
    }
}
