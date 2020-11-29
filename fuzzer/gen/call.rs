use super::*;
use hlang::ast::{Call, Dir, LenInfo, ResValue};
use std::rc::Rc;

#[derive(Default)]
pub(super) struct GenCallContext {
    pub(super) generating_syscall: Option<Rc<Syscall>>,
    pub(super) generated_params: Vec<Value>,
    pub(super) left_len_vals: Vec<(*mut u64, Rc<LenInfo>)>,
}

/// Generate particular syscall.
pub(super) fn gen(ctx: &mut GenContext, syscall: Rc<Syscall>) -> Call {
    ctx.call_ctx.generating_syscall = Some(Rc::clone(&syscall));
    ctx.call_ctx.generated_params.clear();
    for p in syscall.params.iter() {
        let value = param::gen(ctx, Rc::clone(&p.ty), p.dir.unwrap_or(Dir::In));
        ctx.call_ctx.generated_params.push(value);
    }

    let mut ret_value = None;
    if let Some(ret) = syscall.ret.as_ref() {
        let res_value = Rc::new(ResValue::new_res(0, ctx.next_id()));
        ret_value = Some(Value::new_res(
            Dir::Out,
            Rc::clone(ret),
            Rc::clone(&res_value),
        ));
        ctx.add_res(Rc::clone(ret), res_value);
    }

    // calculate the left length parameters.
    if ctx.has_len_call_ctx() {
        len::finish_cal(ctx);
    }

    Call::new(
        syscall,
        ctx.call_ctx.generated_params.split_off(0),
        ret_value,
    )
}
