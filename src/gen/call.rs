use crate::gen::*;
use crate::model::{Call, Dir, LenInfo, ResValue, Value};

use std::sync::Arc;

#[allow(clippy::vec_box)]
#[derive(Default)]
pub(super) struct GenCallContext {
    pub(super) generating_syscall: Option<SyscallRef>,
    pub(super) generated_params: Vec<Box<Value>>, // This is neccessary.
    pub(super) left_len_vals: Vec<(*mut u64, LenInfo)>,
}

/// Generate particular syscall.
pub(super) fn gen(ctx: &mut GenContext, syscall: SyscallRef) -> Call {
    ctx.call_ctx.generating_syscall = Some(syscall);
    ctx.call_ctx.generated_params.clear();
    for p in syscall.params.iter() {
        let value = param::gen(ctx, p.ty, p.dir.unwrap_or(Dir::In));
        ctx.call_ctx.generated_params.push(value);
    }

    let mut ret_value = None;
    if let Some(ret) = syscall.ret {
        let res_value = Arc::new(ResValue::new_res(0, ctx.next_id()));
        ret_value = Some(Value::new(
            Dir::Out,
            ret,
            ValueKind::new_res(Arc::clone(&res_value)),
        ));
        ctx.add_res(ret, res_value);
    }

    // calculate the left length parameters.
    if ctx.has_len_call_ctx() {
        len::finish_cal(ctx);
    }

    Call::new(
        syscall,
        ctx.call_ctx
            .generated_params
            .split_off(0)
            .into_iter()
            .map(|x| *x)
            .collect(),
        ret_value,
    )
}
