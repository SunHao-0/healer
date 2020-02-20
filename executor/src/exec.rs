use core::prog::Prog;
use core::target::Target;
use serde::{Deserialize, Serialize};

pub fn fork_exec(_p: Prog, _t: &Target) -> ExecResult {
    todo!()
}

#[derive(Debug, Clone, Hash, PartialOrd, PartialEq, Eq, Ord, Serialize, Deserialize)]
pub struct Block(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecResult {
    Ok(Vec<Vec<Block>>),
    Err(Error),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Error;
