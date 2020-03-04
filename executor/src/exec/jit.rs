use crate::utils::Waiter;
use core::c::iter_trans;
use core::prog::Prog;
use core::target::Target;
use os_pipe::PipeWriter;
use std::os::unix::io::*;

pub fn exec(_p: &Prog, _t: &Target, _out: &mut PipeWriter, _waiter: Waiter) {
    todo!()
}

fn instrument_prog(p: &Prog, t: &target, data_fd: RawFd, sync_fd: RawFd) -> String {
    const STUB: &str = r#"  "#;
    const VAR_KCOV_FD: &str = "fd";
    const VAR_COVER: &str = "cover";
    const VAR_COVER_LEN: &str = "n";
    const KCOV_SIZE: &str = "1024 * 1024 * 8";
    const VAL_KCOV_OPEN_ERR: i32 = StatusCode::KcovOpenErr as i32;
    const VAL_KCOV_CLOSE_ERR: i32 = StatusCode::KcovCloseErr as i32;
    const VAL_SEND_ERR: i32 = StatusCode::CovSendErr as i32;

    let mut includes = String::from(
        r#"
#include <stdio.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/stat.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <unistd.h>
#include <fcntl.h>
    "#,
    );

    let sync_send = r#"
int sync_send(unsigned long *cover, int len){
    // TODO
}
    "#;

    let macros = r#"
#define KCOV_INIT_TRACE  _IOR('c', 1, unsigned long)
#define KCOV_ENABLE      _IO('c', 100)
#define KCOV_DISABLE     _IO('c', 101)
#define COVER_SIZE       1024*1024
#define KCOV_TRACE_PC    0
    "#;

    let kcov_open = format!(
        r#"
    int fd;
    unsigned long *cover, len;

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
    for s in iter_trans(p, t) {
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
            VAR_KCOV_FD,
            VAR_COVER,
            VAR_COVER_LEN,
            generated_call,
            VAL_KCOV_OPEN_ERR,
            VAL_KCOV_CLOSE_ERR,
            VAL_SEND_ERR
        );
        stmts.push(s);
    }

    todo!()
}

enum StatusCode {
    Ok = 0,
    KcovOpenErr,
    KcovInitErr,
    KcovEnableErr,
    KcovDisableErr,
    CovSendErr,
    MmapErr,
}
