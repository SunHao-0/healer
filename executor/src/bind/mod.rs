/// Binding of picoc
use crate::bind::bind_unix::{
    PicocCleanup, PicocExecute, PicocIncludeAllSystemHeaders, PicocInitialise,
};
use std::ffi::CStr;
use std::os::raw::c_int;
use std::process::exit;

#[allow(dead_code)]
#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[allow(non_upper_case_globals)]
mod bind_unix;

/*
 * struct Picoc
 * void PicocParse(Picoc *pc, const char *FileName, const char *Source, int SourceLen, int RunIt, int CleanupNow, int CleanupSource, int EnableDebugger);
 * void PicocInitialise(Picoc *pc, int StackSize);
 * void PicocCleanup(Picoc *pc);
 * void PicocIncludeAllSystemHeaders(Picoc *pc);
 * #define PicocPlatformSetExitPoint(pc) setjmp((pc)->PicocExitBuf)
 * */

#[derive(Default)]
pub struct Picoc(bind_unix::Picoc);

/// Run cprog by picoc, exit if any error occurs in prog
pub fn picoc_execute(pc: &mut Picoc, mut p: String) -> bool {
    assert!(!p.is_empty());
    const DEFAULT_FILE_NAME: &str = "heal_prog\0";
    // Len of p without \0
    let len = p.len();

    if !p.ends_with('\0') {
        p.push('\0');
    }

    let p = CStr::from_bytes_with_nul(p.as_bytes()).unwrap_or_else(|e| {
        eprintln!("Fail to convert {} to cstr: {}", p, e);
        exit(1);
    });
    let file_name = CStr::from_bytes_with_nul(DEFAULT_FILE_NAME.as_bytes()).unwrap();

    unsafe {
        PicocExecute(
            &mut pc.0 as *mut bind_unix::Picoc,
            file_name.as_ptr(),
            p.as_ptr(),
            len as ::std::os::raw::c_int,
        ) == 0
    }
}

/// Clean up inner state of picoc
pub fn picoc_clean(pc: &mut Picoc) {
    unsafe {
        PicocCleanup(&mut pc.0 as *mut bind_unix::Picoc);
    }
}

/// Include all predefine c header
pub fn picoc_insluce_header(pc: &mut Picoc) {
    unsafe {
        PicocIncludeAllSystemHeaders(&mut pc.0 as *mut bind_unix::Picoc);
    }
}

/// Init state of picoc
pub fn picoc_init(pc: &mut Picoc) {
    const DEFAULT_STACK_SIZE: c_int = 128 * 1024;
    unsafe {
        PicocInitialise(&mut pc.0 as *mut bind_unix::Picoc, DEFAULT_STACK_SIZE);
    }
}
