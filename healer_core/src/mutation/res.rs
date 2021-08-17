//! Mutate value of `resource` type.
use crate::{context::Context, gen::res::gen_res, value::Value, RngType};

pub fn mutate_res(ctx: &mut Context, rng: &mut RngType, val: &mut Value) -> bool {
    let ty = val.ty(ctx.target);
    let new_val = gen_res(ctx, rng, ty, val.dir());
    *val = new_val;
    true
}
