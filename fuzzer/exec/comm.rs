use super::{syz::SyzHandle, CallExecInfo, ExecOpt};
use bytes::{Buf, BufMut};
use hlang::ast::Prog;
use std::io::{Read, Write};
use std::{mem, slice};
use thiserror::Error;

#[derive(Debug, Error)]
pub(super) enum ExecInnerError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("handshake: {0}")]
    HandShake(String),
    #[error("exec magic not match, require: 0xBABC0FFEEBADFACE, received: {0:x}")]
    ExecMagicUnmatch(u64),
    #[error("parse: {0}")]
    Parse(String),
}

const IN_MAGIC: u64 = 0xBADC0FFEEBADFACE;
const OUT_MAGIC: u32 = 0xBADF00D;

#[repr(C)]
struct HandshakeReq {
    magic: u64,
    env_flags: u64, // env flags
    pid: u64,
}
#[repr(C)]
#[derive(Default)]
struct HandshakeReply {
    magic: u32,
}

#[repr(C)]
struct ExecuteReq {
    magic: u64,
    env_flags: u64,  // env flags
    exec_flags: u64, // exec flags
    pid: u64,
    fault_call: u64,
    fault_nth: u64,
    syscall_timeout_ms: u64,
    program_timeout_ms: u64,
    slowdown_scale: u64,
    prog_size: u64,
}

#[repr(C)]
#[derive(Default)]
struct ExecuteReply {
    magic: u32,
    done: u32,
    status: u32,
}

#[repr(C)]
#[derive(Default)]
struct CallReply {
    index: u32, // call index in the program
    num: u32,   // syscall number (for cross-checking)
    errno: u32,
    flags: u32, // see CallFlags
    branch_size: u32,
    block_size: u32,
    comps_size: u32,
}

impl SyzHandle {
    pub(super) fn handshake(&mut self) -> Result<(), ExecInnerError> {
        let req = HandshakeReq {
            magic: IN_MAGIC,
            env_flags: self.env_flags,
            pid: self.pid,
        };
        write_all(&mut self.stdin, &req)?;

        let reply: HandshakeReply = read_exact(&mut self.stdout)?;
        if reply.magic != OUT_MAGIC {
            Err(ExecInnerError::HandShake(format!(
                "reply magic not match, require: {:x}, received: {:x}",
                OUT_MAGIC, reply.magic
            )))
        } else {
            Ok(())
        }
    }

    pub(super) fn exec_inner(
        &mut self,
        opt: ExecOpt,
        in_buf: &[u8], /* stores the serialized prog */
        out_buf: &mut [u8],
    ) -> Result<(), ExecInnerError> {
        let exec_req = ExecuteReq {
            magic: IN_MAGIC,
            env_flags: self.env_flags,
            exec_flags: opt.flags,
            pid: self.pid,
            fault_call: opt.fault_call as u64,
            fault_nth: opt.fault_nth as u64,
            syscall_timeout_ms: 50,
            program_timeout_ms: 5000,
            slowdown_scale: 1,
            prog_size: if self.use_shm { 0 } else { in_buf.len() as u64 },
        };
        write_all(&mut self.stdin, &exec_req)?;
        if !self.use_shm {
            self.stdin.write_all(in_buf)?;
        }

        let exit_status;
        let (completed_calls, mut out) = out_buf.split_at_mut(4);
        let ncalls = cast_from_mut::<u32>(completed_calls);
        loop {
            let exec_reply: ExecuteReply = read_exact(&mut self.stdout)?;
            if exec_reply.magic != OUT_MAGIC {
                return Err(ExecInnerError::ExecMagicUnmatch(exec_reply.magic as u64));
            }
            if exec_reply.done != 0 {
                exit_status = exec_reply.status as i32;
                break;
            }
            let call_reply: CallReply = read_exact(&mut self.stdout)?;
            out.put_slice(cast_to(&call_reply));
            *ncalls += 1;
        }
        if exit_status == 0 {
            Ok(())
        } else {
            Err(ExecInnerError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("exit status: {}", exit_status),
            )))
        }
    }

    pub(super) fn parse_output(
        &self,
        p: &Prog,
        mut out_buf: &[u8],
    ) -> Result<Vec<CallExecInfo>, ExecInnerError> {
        const EXTRA_REPLY_INDEX: u32 = 0xffffffff;
        let ncmd = read_u32(&mut out_buf)
            .ok_or_else(|| ExecInnerError::Parse("failed to read number of calls".to_string()))?;
        let mut info = vec![CallExecInfo::default(); p.calls.len()];
        for i in 0..ncmd {
            let reply: &CallReply = read(&mut out_buf)
                .ok_or_else(|| ExecInnerError::Parse(format!("failed to read call {} reply", i)))?;
            if reply.index != EXTRA_REPLY_INDEX {
                if reply.index as usize > info.len() {
                    return Err(ExecInnerError::Parse(format!(
                        "bad call {} index {}/{}",
                        i,
                        reply.index,
                        info.len()
                    )));
                }
                let sid = p.calls[reply.index as usize].meta.id;
                if sid != reply.num as usize {
                    return Err(ExecInnerError::Parse(format!(
                        "wrong call {} num {}/{}",
                        i, reply.num, sid
                    )));
                }
                let call_info = &mut info[reply.index as usize];
                if call_info.flags != 0 || !call_info.branches.is_empty() {
                    return Err(ExecInnerError::Parse(format!(
                        "duplicate reply for call {}/{}/{}",
                        i, reply.index, reply.num
                    )));
                }

                if reply.comps_size != 0 {
                    return Err(ExecInnerError::Parse(format!(
                        "comparison collected for call {}/{}/{}",
                        i, reply.index, reply.num
                    )));
                }
                call_info.flags = reply.flags;
                call_info.errno = reply.errno as i32;
                if reply.branch_size != 0 {
                    let br = read_u32_slice(&mut out_buf, reply.branch_size as usize).ok_or_else(
                        || {
                            ExecInnerError::Parse(format!(
                                "call {}/{}/{}: signal overflow: {}/{}",
                                i,
                                reply.index,
                                reply.num,
                                reply.branch_size,
                                out_buf.len()
                            ))
                        },
                    )?;
                    // TODO do not copy.
                    call_info.branches.extend(br);
                }
                if reply.block_size != 0 {
                    let bk = read_u32_slice(&mut out_buf, reply.block_size as usize).ok_or_else(
                        || {
                            ExecInnerError::Parse(format!(
                                "call {}/{}/{}: cover overflow: {}/{}",
                                i,
                                reply.index,
                                reply.num,
                                reply.block_size,
                                out_buf.len()
                            ))
                        },
                    )?;
                    // TODO do not copy.
                    call_info.blocks.extend(bk);
                }
            }
        }
        Ok(info)
    }
}

fn read_u32(buf: &mut &[u8]) -> Option<u32> {
    if buf.remaining() >= 4 {
        Some(buf.get_u32_le())
    } else {
        None
    }
}

fn read_u32_slice<'a>(buf: &mut &'a [u8], len: usize) -> Option<&'a [u32]> {
    let l = len * mem::size_of::<u32>();
    if l <= buf.len() {
        let ret = unsafe { slice::from_raw_parts(buf.as_ptr() as *const u32, len) };
        buf.advance(l);
        Some(ret)
    } else {
        None
    }
}

fn read<'a, T: Sized>(buf: &mut &'a [u8]) -> Option<&'a T> {
    let sz = mem::size_of::<T>();
    if buf.len() >= sz {
        let buf0 = &buf[0..sz];
        let v = cast_from(buf0);
        buf.advance(sz);
        Some(v)
    } else {
        None
    }
}

fn read_exact<T: Default + Sized, R: Read>(mut r: R) -> Result<T, std::io::Error> {
    let mut v = T::default();
    let data = cast_to_mut(&mut v);
    r.read_exact(data)?;
    Ok(v)
}

fn write_all<T: Sized, W: Write>(mut w: W, v: &T) -> Result<(), std::io::Error> {
    let data = cast_to(v);
    w.write_all(data)
}

fn cast_to<T: Sized>(v: &T) -> &[u8] {
    let ptr = (v as *const T).cast::<u8>();
    let len = mem::size_of::<T>();
    unsafe { slice::from_raw_parts(ptr, len) }
}

fn cast_to_mut<T: Sized>(v: &mut T) -> &mut [u8] {
    let ptr = (v as *mut T).cast::<u8>();
    let len = mem::size_of::<T>();
    unsafe { slice::from_raw_parts_mut(ptr, len) }
}

fn cast_from<T: Sized>(v: &[u8]) -> &T {
    assert_eq!(v.len(), mem::size_of::<T>());
    let ptr = v.as_ptr() as *const T;
    unsafe { ptr.as_ref().unwrap() }
}

fn cast_from_mut<T: Sized>(v: &mut [u8]) -> &mut T {
    assert_eq!(v.len(), mem::size_of::<T>());
    let ptr = v.as_ptr() as *mut T;
    unsafe { ptr.as_mut().unwrap() }
}
