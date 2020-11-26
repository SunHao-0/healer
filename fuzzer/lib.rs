// Use nightly version is a bad idea, because it sometimes generates the wrong code and the implementation of the borrow checker is also incorrect.
// #![feature(test)]. Require nightly.
// extern crate test;
#![allow(unused_variables)]
#![allow(dead_code)]
extern crate bv;

pub mod exec;
pub mod fuzz;
pub mod gen;
pub mod target;
