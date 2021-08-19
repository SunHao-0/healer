//! Mutate value of `resource` type.
#[cfg(debug_assertions)]
use crate::mutation::call::display_value_diff;
use crate::{
    context::Context,
    gen::{current_builder, res::gen_res},
    ty::Dir,
    value::Value,
    RngType,
};

pub fn mutate_res(ctx: &mut Context, rng: &mut RngType, val: &mut Value) -> bool {
    if val.dir() == Dir::Out {
        return false; // do not change output resource id
    }

    let ty = val.ty(ctx.target);
    let new_val = gen_res(ctx, rng, ty, val.dir());
    // update call's used res
    let ty = ty.checked_as_res();
    if let Some(id) = new_val.checked_as_res().res_val_id() {
        current_builder(|b| {
            let ent = b.used_res.entry(ty.res_name().clone()).or_default();
            ent.insert(id);
        })
    }
    verbose!(
        "mutate_res: {}",
        display_value_diff(val, &new_val, ctx.target)
    );
    let mutated = new_val != *val;
    *val = new_val;

    mutated
}
