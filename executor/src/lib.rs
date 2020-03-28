#[macro_use]
extern crate thiserror;
#[macro_use]
extern crate nix;
#[macro_use]
extern crate maplit;

use core::target::Target;
use std::fs::{create_dir_all, write};
use std::io::ErrorKind;
use std::io::{Read, Write};

#[macro_use]
mod utils;
pub mod cover;
pub mod exec;
pub mod transfer;

pub use exec::{ExecResult, Reason};
use std::path::PathBuf;

/// Read prog from conn, translate by target, run the translated test program.
pub fn exec_loop<T: Read + Write>(_t: Target, mut conn: T) {
    if cfg!(feature = "jit") {
        prepare_env();
    }

    loop {
        let p = transfer::recv_prog(&mut conn)
            .unwrap_or_else(|e| exits!(exitcode::SOFTWARE, "Fail to recv:{}", e));

        let result = exec::fork_exec(p, &_t);

        transfer::send(&result, &mut conn)
            .unwrap_or_else(|e| exits!(exitcode::SOFTWARE, "Fail to Send {:?}:{}", result, e));
    }
}

fn prepare_env() {
    let float_h = include_str!("../tcc-0.9.27/include/float.h");
    let stdarg_h = include_str!("../tcc-0.9.27/include/stdarg.h");
    let stdbool_h = include_str!("../tcc-0.9.27/include/stdbool.h");
    let stddef_h = include_str!("../tcc-0.9.27/include/stddef.h");
    let varargs_h = include_str!("../tcc-0.9.27/include/varargs.h");
    let tcc_include = PathBuf::from("healer/runtime/tcc/include");
    if let Err(e) = create_dir_all(&tcc_include) {
        if let ErrorKind::AlreadyExists = e.kind() {
            ()
        } else {
            panic!("Fail to create: {} :{}", tcc_include.display(), e);
        }
    }
    write(tcc_include.join("float.h"), float_h).unwrap();
    write(tcc_include.join("stdarg.h"), stdarg_h).unwrap();
    write(tcc_include.join("stdbool.h"), stdbool_h).unwrap();
    write(tcc_include.join("stddef.h"), stddef_h).unwrap();
    write(tcc_include.join("varargs.h"), varargs_h).unwrap();
}
