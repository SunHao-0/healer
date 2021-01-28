use crate::gen::{context::GenContext, len, param};
use crate::model::{Call, Dir, LenInfo, ResValue, SyscallRef, Value, ValueKind};

#[allow(clippy::vec_box)]
#[derive(Default)]
pub(crate) struct GenCallContext {
    pub(crate) generating_syscall: Option<SyscallRef>,
    pub(crate) generated_params: Vec<Box<Value>>, // This is neccessary.
    pub(crate) left_len_vals: Vec<(*mut u64, LenInfo)>,
    pub(crate) val_cnt: usize,
    pub(crate) res_cnt: usize,
}

/// Generate particular syscall.
pub(super) fn gen(ctx: &mut GenContext, syscall: SyscallRef) -> Call {
    ctx.call_ctx.generating_syscall = Some(syscall);
    ctx.call_ctx.generated_params.clear();
    ctx.call_ctx.val_cnt = 0;

    for p in syscall.params.iter() {
        let value = param::gen(ctx, p.ty, p.dir.unwrap_or(Dir::In));
        ctx.call_ctx.generated_params.push(value);
    }

    let mut ret_value = None;
    if let Some(ret) = syscall.ret {
        let mut res_val = Box::new(ResValue::new_res(0, ctx.next_id()));
        ctx.add_res(ret, &mut *res_val);
        ret_value = Some(Value::new(Dir::Out, ret, ValueKind::new_res(res_val)));
    }

    // calculate the left length parameters.
    if ctx.has_len_call_ctx() {
        len::finish_cal(ctx);
    }

    let args = ctx
        .call_ctx
        .generated_params
        .split_off(0)
        .into_iter()
        .map(|x| *x)
        .collect();

    Call {
        meta: syscall,
        args,
        ret: ret_value,
        val_cnt: ctx.call_ctx.val_cnt,
        res_cnt: ctx.call_ctx.res_cnt,
    }
}
