/// Prog to c
///
/// This module translate internal prog representation to c script.
/// It does type mapping, varibles declarint ..
use crate::prog::Prog;
use crate::target::Target;

/// Placeholder
pub struct CProg;

pub fn translate(p: &Prog, t: &Target) -> CProg {
    let _g = &t.groups[&p.gid];
    todo!()
}
