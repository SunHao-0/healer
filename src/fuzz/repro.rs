//! Reproduce and cook a crash
use crate::model::Prog;

pub struct Report;

pub fn repro<T: AsRef<Prog>>(_history: &[T]) -> Report {
    // extract the crash info
    // try to repro with past test case
    // generate the target test case in text format
    // save above info
    todo!()
}
