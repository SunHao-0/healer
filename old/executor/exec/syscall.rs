use crate::utils::Waiter;
use core::prog::Prog;
use core::target::Target;
use os_pipe::PipeWriter;

#[cfg(feature = "kcov")]
pub fn exec(_p: &Prog, _t: &Target, _out: &mut PipeWriter, _waiter: Waiter) {
    todo!()
}

#[cfg(not(feature = "kcov"))]
pub fn exec(_p: &Prog, _t: &Target) {}

pub fn bg_exec(_p: &Prog, _t: &Target) {
    todo!()
}
