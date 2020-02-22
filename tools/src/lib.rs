#[macro_use]
extern crate pest_derive;
#[macro_use]
extern crate thiserror;

use core::target::Target;
use fots::types::Items;
use std::fs::read;
use std::path::PathBuf;
use std::process::exit;

pub mod def2flag;

pub fn load_target(items: &PathBuf) -> Target {
    let items = read(items).unwrap_or_else(|e| {
        eprintln!("Fail to read {:?}: {}", items, e);
        exit(exitcode::NOINPUT)
    });
    let items: Items = bincode::deserialize(&items).unwrap_or_else(|e| {
        eprintln!("Fail to deserialize: {}", e);
        exit(exitcode::DATAERR)
    });
    Target::from(items)
}
