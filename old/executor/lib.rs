#[macro_use]
extern crate thiserror;
#[macro_use]
extern crate nix;
#[macro_use]
extern crate maplit;

use core::target::Target;
use std::io::{Read, Write};

#[macro_use]
#[allow(dead_code)]
mod utils;
pub mod cover;
#[allow(unused_imports, unused_mut, dead_code)]
pub mod exec;
pub mod transfer;

pub use exec::{ExecResult, Reason};

pub struct Config {
    pub memleak_check: bool,
    pub concurrency: bool,
}

/// Read prog from conn, translate by target, run the translated test program.
pub fn exec_loop<T: Read + Write>(t: Target, mut conn: T, conf: Config) {
    loop {
        let p = transfer::recv_prog(&mut conn)
            .unwrap_or_else(|e| exits!(exitcode::SOFTWARE, "Fail to recv:{}", e));

        let result = exec::fork_exec(p, &t, &conf);

        transfer::send(&result, &mut conn)
            .unwrap_or_else(|e| exits!(exitcode::SOFTWARE, "Fail to Send {:?}:{}", result, e));
    }
}
