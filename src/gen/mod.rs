use crate::model::*;
use crate::targets::Target;
use rand::prelude::*;
use std::sync::Arc;

use self::context::ProgContext;

// Syscall selection.
pub mod select;
// Argument value generation.
pub mod param;
// Call generation.
pub mod call;
// Length type calculation. It's here because length calculation is inter-params.
pub mod len;

pub mod context;

pub(crate) const MAX_LEN: usize = 32;
pub(crate) const MIN_LEN: usize = 4;

/// Gnerate test case based current value pool and test target.
pub fn gen(target: &Target) -> Prog {
    let mut ctx = ProgContext::new(target);
    gen_inner(&mut ctx)
}

pub fn gen_seq(target: &Target, seq: &[SyscallRef]) -> Prog {
    let mut ctx = ProgContext::new(target);
    let mut calls = Vec::new();
    for call in seq {
        calls.push(call::gen(&mut ctx, call));
    }
    Prog::new(calls)
}

fn gen_inner(ctx: &mut ProgContext) -> Prog {
    let mut calls: Vec<Call> = Vec::new();
    while !should_stop(calls.len()) {
        append(ctx, &mut calls);
    }
    Prog::new(calls)
}

pub(crate) fn append(ctx: &mut ProgContext, calls: &mut Vec<Call>) {
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
