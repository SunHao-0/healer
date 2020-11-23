use super::*;
use hlang::ast::{Call, Dir, ResValue};
use std::rc::Rc;

/// Generate particular syscall.
pub(super) fn gen(ctx: &mut GenContext, syscall: Rc<Syscall>) -> Call {
    let mut param_values = Vec::new();
    for p in syscall.params.iter() {
        let value = param::gen(ctx, Rc::clone(&p.ty), p.dir.unwrap_or(Dir::In));
        param_values.push(value);
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
    Call::new(syscall, param_values, ret_value)
}
