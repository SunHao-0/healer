#[macro_use]
extern crate thiserror;
#[macro_use]
extern crate nix;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate maplit;

use core::target::Target;
use std::io::{Read, Write};

#[macro_use]
mod utils;
pub mod cover;
pub mod exec;
pub mod transfer;

pub use exec::{ExecResult, Reason};

/// Read prog from conn, translate by target, run the translated test program.
pub fn exec_loop<T: Read + Write>(_t: Target, mut conn: T) {
    loop {
        let p = transfer::recv_prog(&mut conn)
            .unwrap_or_else(|e| exits!(exitcode::SOFTWARE, "Fail to recv:{}", e));

        let result = exec::fork_exec(p, &_t);

        transfer::send(&result, &mut conn)
            .unwrap_or_else(|e| exits!(exitcode::SOFTWARE, "Fail to Send {:?}:{}", result, e));
    }
}
