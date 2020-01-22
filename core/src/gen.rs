use std::collections::HashMap;

use fots::types::GroupId;

use crate::analyze::RTable;
use crate::prog::Prog;
use crate::target::Target;

pub struct Config {
    len: usize
}


pub fn gen(t: &Target, rs: &HashMap<GroupId, RTable>, conf: &Config) -> Prog {
    todo!()
}


fn choose_seq(rs: &RTable, conf: Config) -> Vec<usize> {
    let mut seq = Vec::new();
}