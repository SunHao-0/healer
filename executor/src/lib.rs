#[macro_use]
extern crate thiserror;
#[macro_use]
extern crate nix;

use core::target::Target;
use std::io::{Read, Write};
use std::process::exit;

#[macro_use]
pub mod utils;
pub mod cover;
pub mod exec;
pub mod transfer;

pub use exec::{Block, Error, ExecResult};

/// Read prog from conn, translate by target, run the translated test program.
pub fn exec_loop<T: Read + Write>(_t: Target, mut conn: T) {
    loop {
        let p = transfer::recv_prog(&mut conn).unwrap_or_else(|e| {
            eprintln!("Recving:{}", e);
            exit(exitcode::DATAERR)
        });

        let result = exec::fork_exec(p, &_t);

        transfer::send(&result, &mut conn).unwrap_or_else(|e| {
            eprintln!("Send {:?}:{}", result, e);
            exit(exitcode::DATAERR)
        });
    }
}
