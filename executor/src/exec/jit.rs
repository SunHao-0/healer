use crate::utils::Waiter;
use core::c;
use core::c::cths::CTHS;
use core::c::iter_trans;
use core::prog::Prog;
use core::target::Target;
use os_pipe::PipeWriter;
use std::fmt::Write;
use std::os::raw::c_int;
use std::os::unix::io::*;
use std::process::exit;
use tcc::Symbol;

pub fn exec(p: &Prog, t: &Target, out: &mut PipeWriter, waiter: Waiter) {
    let p = instrument_prog(p, t, out.as_raw_fd(), waiter.as_raw_fd()).unwrap_or_else(|e| {
        eprintln!("{}", e);
        exit(exitcode::SOFTWARE);
    });

    let mut cc = tcc::Tcc::new();
    cc.add_sysinclude_path("/usr/local/lib/tcc/include")
        .unwrap();
    cc.add_library_path("/usr/local/lib/tcc").unwrap();
    cc.set_output_type(tcc::OutputType::Memory)
        .unwrap_or_else(|_| exits!(exitcode::SOFTWARE, "Fail to set up jit"));
    cc.set_error_func(Some(Box::new(|e| {
        if e.contains("error") {
            eprintln!("{}", e);
        }
    })));
    cc.compile_string(&p)
        .unwrap_or_else(|_| exits!(exitcode::SOFTWARE, "Fail to compile generated prog: {}", p));
    let mut p = cc
        .relocate()
        .unwrap_or_else(|_| exits!(exitcode::SOFTWARE, "Fail to relocate compiled prog"));
    let execute: fn() -> c_int = {
        let symbol = p.get_symbol("execute").unwrap();
        unsafe { std::mem::transmute(symbol.as_ptr()) }
    };

    let code = execute();
    if code == 0 {
        exit(exitcode::OK)
    } else {
        exits!(
            exitcode::SOFTWARE,
            "Fail to execute: {:?}",
            StatusCode::from(code)
        )
    }
}

pub fn bg_exec(p: &Prog, t: &Target) {
    let p = c::to_prog(p, t);
    if !p.is_empty() {
        let mut cc = tcc::Tcc::new();
        cc.add_sysinclude_path("/usr/local/lib/tcc/include")
            .unwrap();
        cc.add_library_path("/usr/local/lib/tcc").unwrap();
        cc.set_output_type(tcc::OutputType::Memory).unwrap();
        cc.compile_string(&p).unwrap();
        cc.run(&["healer-executor-bg-exec"]).unwrap()
    }
}

pub fn instrument_prog(p: &Prog, t: &Target, data_fd: RawFd, sync_fd: RawFd) -> Result<String, String> {
    let mut includes = hashset! {
        "stdio.h",
        "stddef.h",
        "stdint.h",
        "stdlib.h",
        "sys/types.h",
        "sys/stat.h",
        "sys/ioctl.h",
        "sys/mman.h",
        "unistd.h",
        "fcntl.h",
        "string.h"
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
        let call_name = t.fn_of(p.calls[i].fid).call_name.clone();
        let header = if let Some(h) = CTHS.get(&call_name as &str) {
            h
        } else {
            return Err(format!("Fail to instrument: {}", call_name));
        };
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
