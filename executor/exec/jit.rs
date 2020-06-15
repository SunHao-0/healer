use crate::utils::Waiter;
use core::c;
use core::c::cths::CTHS;
use core::c::iter_trans;
use core::prog::Prog;
use core::target::Target;
use os_pipe::PipeWriter;
use std::ffi::CString;
use std::fmt::Write;
use std::fs::{create_dir_all, write};
use std::io::ErrorKind;
use std::os::raw::c_int;
use std::os::unix::io::*;
use std::path::PathBuf;
use std::process::exit;
use tcc::{Context, Guard};

#[cfg(feature = "kcov")]
pub fn exec(p: &Prog, t: &Target, out: &mut PipeWriter, waiter: Waiter) {
    prepare_env();
    let p = {
        instrument_prog(p, t, out.as_raw_fd(), waiter.as_raw_fd()).unwrap_or_else(|e| {
            eprintln!("{}", e);
            exit(exitcode::SOFTWARE);
        })
    };

    let p = CString::new(p.as_bytes()).unwrap();
    let sym = CString::new("execute").unwrap();

    let mut g = Guard::new().unwrap();
    let mut cc = new_tcc(&mut g);
    cc.compile_string(&p).unwrap_or_else(|_| {
        exits!(
            exitcode::SOFTWARE,
            "Fail to compile generated prog: {:?}",
            p
        )
    });
    let mut p = cc
        .relocate()
        .unwrap_or_else(|_| exits!(exitcode::SOFTWARE, "Fail to relocate compiled prog"));
    let execute: fn() -> c_int = unsafe {
        let symbol = p.get_symbol(&sym).unwrap();
        std::mem::transmute(symbol)
    };

    let code = execute();
    if code != 0 {
        exits!(
            exitcode::SOFTWARE,
            "Fail to execute: {:?}",
            StatusCode::from(code)
        )
    }
}

#[cfg(not(feature = "kcov"))]
pub fn exec(p: &Prog, t: &Target) {
    prepare_env();
    let p = c::to_prog(p, t);
    let p = CString::new(p.as_bytes()).unwrap();
    let sym = CString::new("main").unwrap();

    let mut g = Guard::new().unwrap();
    let mut cc = new_tcc(&mut g);
    cc.compile_string(&p).unwrap_or_else(|_| {
        exits!(
            exitcode::SOFTWARE,
            "Fail to compile generated prog: {:?}",
            p
        )
    });
    let mut p = cc
        .relocate()
        .unwrap_or_else(|_| exits!(exitcode::SOFTWARE, "Fail to relocate compiled prog"));
    let execute: fn() -> c_int = unsafe {
        let symbol = p.get_symbol(&sym).unwrap();
        std::mem::transmute(symbol)
    };
    execute();
}

pub fn bg_exec(p: &Prog, t: &Target) {
    let p = c::to_prog(p, t);
    if !p.is_empty() {
        let p = CString::new(p.as_bytes()).unwrap();
        let sym = CString::new("main").unwrap();
        let mut g = Guard::new().unwrap();
        let mut cc = new_tcc(&mut g);
        cc.compile_string(&p).unwrap();
        let mut p = cc
            .relocate()
            .unwrap_or_else(|_| exits!(exitcode::SOFTWARE, "Fail to relocate compiled prog"));
        let execute: fn() -> c_int = unsafe {
            let symbol = p.get_symbol(&sym).unwrap();
            std::mem::transmute(symbol)
        };
        execute();
    }
}

pub fn instrument_prog(
    p: &Prog,
    t: &Target,
    data_fd: RawFd,
    sync_fd: RawFd,
) -> Result<String, String> {
    let mut includes = hashset! {
        "stdio.h".to_string(),
         "stddef.h".to_string(),
        "stdint.h".to_string(),
        "stdlib.h".to_string(),
        "sys/types.h".to_string(),
        "sys/stat.h".to_string(),
        "sys/ioctl.h".to_string(),
        "sys/mman.h".to_string(),
        "unistd.h".to_string(),
        "fcntl.h".to_string(),
        "string.h".to_string()
    };

    let macros = r#"
#define KCOV_INIT_TRACE  _IOR('c', 1, unsigned long)
#define KCOV_ENABLE      _IO('c', 100)
#define KCOV_DISABLE     _IO('c', 101)
#define COVER_SIZE       1024*1024
#define KCOV_TRACE_PC    0
    "#;

    let sync_send = format!(
        r#"
int sync_send(unsigned long *cover, uint32_t len){{
    if (len == 0){{
        return 0;
    }}
    char *cover_ = (void*)(cover + 1);
    int l2;
    int event_fd = {}, data_fd = {};
    char l[4];
    char event[8];

    memcpy(l, &len, 4);
    if (write(data_fd, l, 4) == -1){{
        return -1;
    }}

    len = len * sizeof(unsigned long);
    while(1){{
        l2 = write(data_fd, cover_, len);
        if(l2 == -1){{
            return -1;
        }}
        len -= l2;
        cover_ += l2;

        if (len == 0){{
            break;
        }}
    }}
    if(read(event_fd, event, 8) == -1){{
        return -1;
    }}
    return 0;
}}"#,
        sync_fd, data_fd
    );

    let kcov_open = format!(
        r#"
    int fd;
    unsigned long *cover;
    uint32_t len = 0;

    fd = open("/sys/kernel/debug/kcov", O_RDWR);
    if (fd == -1)
            return {};
    if (ioctl(fd, KCOV_INIT_TRACE, COVER_SIZE))
            return {};
    cover = (unsigned long*)mmap(NULL, COVER_SIZE * sizeof(unsigned long),
                                 PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if ((void*)cover == MAP_FAILED)
            return {};
    "#,
        StatusCode::KcovOpenErr as i32,
        StatusCode::KcovInitErr as i32,
        StatusCode::MmapErr as i32
    );

    let mut stmts = Vec::new();
    for (i, s) in iter_trans(p, t).enumerate() {
        let fn_info = t.fn_of(p.calls[i].fid);
        let call_name = fn_info.call_name.clone();
        let mut header = if let Some(h) = CTHS.get(&call_name as &str) {
            h.iter().map(|h| (*h).to_string()).collect::<Vec<_>>()
        } else {
            Default::default()
        };
        if let Some(attr) = fn_info.get_attr("inc") {
            if let Some(vals) = attr.vals.as_ref() {
                for val in vals {
                    header.push(val.to_string());
                }
            }
        }

        includes.extend(header);

        let generated_call = s.to_string();
        let s = format!(
            r#"
    if (ioctl(fd, KCOV_ENABLE, KCOV_TRACE_PC))
            return {};
    cover[0] = 0;
    {}
    len = cover[0];
    if (ioctl(fd, KCOV_DISABLE, 0))
            return {};
    if (sync_send(cover, len) == -1)
        return {};"#,
            StatusCode::KcovEnableErr as i32,
            generated_call,
            StatusCode::KcovDisableErr as i32,
            StatusCode::CovSendErr as i32
        );
        stmts.push(s);
    }

    let clean = format!(
        r#"
    if (munmap(cover, COVER_SIZE * sizeof(unsigned long)))
            return {};
    if (close(fd))
            return {};
    return {};
    "#,
        StatusCode::MmapErr as i32,
        StatusCode::KcovCloseErr as i32,
        StatusCode::Ok as i32
    );

    let execute = {
        let mut buf = String::new();
        writeln!(buf, "{}", kcov_open).unwrap();
        for s in stmts {
            writeln!(buf, "{}", s).unwrap();
        }
        writeln!(buf, "{}", clean).unwrap();
        format!("int execute(){{\n{}}}", buf)
    };

    let mut buf = String::new();
    writeln!(buf, "#define _GNU_SOURCE").unwrap();

    for header in includes.into_iter() {
        writeln!(buf, "#include<{}>", header).unwrap();
    }
    writeln!(buf, "{}", macros).unwrap();
    writeln!(buf, "{}", sync_send).unwrap();
    writeln!(buf, "{}", execute).unwrap();
    Ok(buf)
}

#[derive(Debug)]
enum StatusCode {
    Ok = 0,
    KcovOpenErr,
    KcovCloseErr,
    KcovInitErr,
    KcovEnableErr,
    KcovDisableErr,
    CovSendErr,
    MmapErr,
}

impl From<i32> for StatusCode {
    fn from(val: i32) -> Self {
        use StatusCode::*;

        match val {
            0 => Ok,
            1 => KcovOpenErr,
            2 => KcovCloseErr,
            3 => KcovInitErr,
            4 => KcovEnableErr,
            5 => KcovDisableErr,
            6 => CovSendErr,
            7 => MmapErr,
            _ => unreachable!(),
        }
    }
}

const TCC_INCLUDE: &str = "/usr/local/include/healer/tcc";

fn new_tcc<'a, 'b>(g: &'a mut Guard) -> Context<'a, 'b> {
    let mut cc = tcc::Context::new(g).unwrap();
    cc.add_sys_include_path(TCC_INCLUDE);
    if cfg!(target_os = "linux") {
        cc.add_sys_include_path("/usr/include");
        cc.add_sys_include_path("/usr/local/include");
        cc.add_library_path("/usr/lib");
        cc.add_library_path("/usr/local/lib");
    }
    cc.set_output_type(tcc::OutputType::Memory);
    // ignore wraning
    cc.set_call_back(|e| {
        let e = e.to_str().unwrap();
        if e.contains("error") {
            eprintln!("{}", e);
        }
    });
    cc
}

fn prepare_env() {
    let float_h = include_str!("../tcc-0.9.27/include/float.h");
    let stdarg_h = include_str!("../tcc-0.9.27/include/stdarg.h");
    let stdbool_h = include_str!("../tcc-0.9.27/include/stdbool.h");
    let stddef_h = include_str!("../tcc-0.9.27/include/stddef.h");
    let varargs_h = include_str!("../tcc-0.9.27/include/varargs.h");
    let tcc_include = PathBuf::from(TCC_INCLUDE);
    if let Err(e) = create_dir_all(&tcc_include) {
        if ErrorKind::AlreadyExists != e.kind() {
            panic!("Fail to create: {} :{}", tcc_include.display(), e);
        } else {
            return;
        }
    }
    write(tcc_include.join("float.h"), float_h).unwrap();
    write(tcc_include.join("stdarg.h"), stdarg_h).unwrap();
    write(tcc_include.join("stdbool.h"), stdbool_h).unwrap();
    write(tcc_include.join("stddef.h"), stddef_h).unwrap();
    write(tcc_include.join("varargs.h"), varargs_h).unwrap();
}
