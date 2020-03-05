/// Binding of picoc
use std::ffi::CStr;
use std::os::raw::c_int;
use std::process::exit;

include!(concat!(env!("OUT_DIR"), "/bind_unix.rs"));

#[derive(Default)]
pub struct PicocWrapper(Picoc);

/// Run cprog by picoc, exit if any error occurs in prog
pub fn picoc_execute(pc: &mut PicocWrapper, mut p: String) -> bool {
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
            &mut pc.0 as *mut Picoc,
            file_name.as_ptr(),
            p.as_ptr(),
            len as ::std::os::raw::c_int,
        ) == 0
    }
}

/// Clean up inner state of picoc
pub fn picoc_clean(pc: &mut PicocWrapper) {
    unsafe {
        PicocCleanup(&mut pc.0 as *mut Picoc);
    }
}

/// Include all predefine c header
pub fn picoc_insluce_header(pc: &mut PicocWrapper) {
    unsafe {
        PicocIncludeAllSystemHeaders(&mut pc.0 as *mut Picoc);
    }
}

/// Init state of picoc
pub fn picoc_init(pc: &mut PicocWrapper) {
    const DEFAULT_STACK_SIZE: c_int = 128 * 1024;
    unsafe {
        PicocInitialise(&mut pc.0 as *mut Picoc, DEFAULT_STACK_SIZE);
    }
}
