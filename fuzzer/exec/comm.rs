use bytes::BufMut;
use std::io::{Read, Write};
use std::{fmt, mem, slice};

use super::syz::{EnvFlags, ExecOpt};

#[derive(Debug)]
pub enum CommError {
    Io(std::io::Error),
    HandShake(String),
    ExecMagicUnmatch(u64),
}

impl fmt::Display for CommError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommError::Io(ref err) => write!(f, "Io: {}", err),
            CommError::HandShake(ref err) => write!(f, "handshake: {}", err),
            CommError::ExecMagicUnmatch(err) => write!(
                f,
                "exec magic not match, require: {}, received: {}",
                OUT_MAGIC, err
            ),
        }
    }
}

impl From<std::io::Error> for CommError {
    fn from(err: std::io::Error) -> Self {
        CommError::Io(err)
    }
}

const IN_MAGIC: u64 = 0xBABC0FFEEBADFACE;
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

pub fn handshake<T: Write, R: Read>(
    mut w: T,
    mut r: R,
    env_flags: u64,
    pid: u64,
) -> Result<(), CommError> {
    let req = HandshakeReq {
        magic: IN_MAGIC,
        env_flags,
        pid,
    };
    let req_data = cast_to(&req);
    w.write_all(req_data)?;

    let mut reply_data = vec![0u8; mem::size_of::<HandshakeReply>()];
    r.read_exact(&mut reply_data)?;
    let reply = cast_from::<HandshakeReply>(&reply_data[..]);

    if reply.magic != OUT_MAGIC {
        Err(CommError::HandShake(format!(
            "reply magic not match, require: {:x}, received: {:x}",
            OUT_MAGIC, reply.magic
        )))
    } else {
        Ok(())
    }
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

#[allow(unused_assignments)]
pub fn exec<T: Write, R: Read>(
    mut w: T,
    mut r: R,
    opt: ExecOpt,
    pid: u64,
    env_flags: EnvFlags,
    in_buf: &[u8],
    out_buf: &mut [u8],
) -> Result<(), CommError> {
    let exec_req = ExecuteReq {
        magic: IN_MAGIC,
        env_flags,
        exec_flags: opt.flags,
        pid: pid,
        fault_call: opt.fault_call as u64,
        fault_nth: opt.fault_nth as u64,
        syscall_timeout_ms: 50,
        program_timeout_ms: 5000,
        slowdown_scale: 1,
        prog_size: if opt.use_shm { 0 } else { in_buf.len() as u64 },
    };
    let req_data = cast_to(&exec_req);
    w.write_all(req_data)?;
    if !opt.use_shm {
        w.write_all(in_buf)?;
    }

    let mut exit_status = -1;
    let (completed_calls, mut out) = out_buf.split_at_mut(4);
    let ncalls = cast_from_mut::<u32>(completed_calls);
    loop {
        let exec_reply: ExecuteReply = read_exact(&mut r)?;
        if exec_reply.magic != OUT_MAGIC {
            return Err(CommError::ExecMagicUnmatch(exec_reply.magic as u64));
        }
        if exec_reply.done != 0 {
            exit_status = exec_reply.status as i32;
            break;
        }
        let call_reply: CallReply = read_exact(&mut r)?;
        out.put_slice(cast_to(&call_reply));
        *ncalls += 1;
    }

    #[allow(unreachable_code)]
    if exit_status == 0 {
        todo!()
    } else if exit_status == -1 {
        exit_status = todo!()
    }
    todo!()
}

fn read_exact<T: Default + Sized, R: Read>(mut r: R) -> Result<T, std::io::Error> {
    let mut v = T::default();
    let data = cast_to_mut(&mut v);
    r.read_exact(data)?;
    Ok(v)
}
