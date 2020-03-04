use crate::cover;
use crate::utils::Waiter;
use byteorder::*;
use core::c::iter_trans;
use core::prog::Prog;
use core::target::Target;
use os_pipe::PipeWriter;
use std::io::Write;
use std::process::exit;

#[allow(dead_code)]
#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[allow(non_upper_case_globals)]
#[allow(clippy::unreadable_literal)]
#[allow(clippy::redundant_static_lifetimes)]
#[allow(clippy::transmute_ptr_to_ptr)]
mod bind;

mod picoc;
use picoc::Picoc;

pub fn exec(p: &Prog, t: &Target, out: &mut PipeWriter, waiter: Waiter) {
    let mut picoc = Picoc::default();
    let mut handle = cover::open();
    let mut success = false;

    for stmts in iter_trans(p, t) {
        let covs = handle.collect(|| {
            success = picoc.execute(stmts.to_string());
        });
        if success {
            send_covs(covs, out);
            waiter.wait()
        } else {
            exit(exitcode::SOFTWARE)
        }
    }
}

fn send_covs<T: Write>(covs: &[usize], out: &mut T) {
    use byte_slice_cast::*;
    assert!(!covs.is_empty());

    out.write_u32::<NativeEndian>(covs.len() as u32)
        .unwrap_or_else(|e| exits!(exitcode::IOERR, "Fail to write len of covs: {}", e));
    out.write_all(covs.as_byte_slice())
        .unwrap_or_else(|e| exits!(exitcode::IOERR, "Fail to write covs: {}", e));
}
