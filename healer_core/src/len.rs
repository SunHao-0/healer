use crate::{prog::Call, syscall::Syscall, target::Target, value::Value};

pub fn calculate_len_call(target: &Target, call: &mut Call) {
    let syscall = target.syscall_of(call.sid());
    calculate_len(target, syscall, call.args_mut())
}

pub fn calculate_len(_target: &Target, _syscall: &Syscall, _args: &mut [Value]) {
    // todo!()
}
