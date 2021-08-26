//! Communication messages with syz-executor

pub const IN_MAGIC: u64 = 0xBADC0FFEEBADFACE;
pub const OUT_MAGIC: u32 = 0xBADF00D;

#[repr(C)]
#[derive(Default, Debug)]
pub struct HandshakeReq {
    pub magic: u64,
    pub env_flags: u64, // env flags
    pub pid: u64,
}
#[repr(C)]
#[derive(Default, Debug)]
pub struct HandshakeReply {
    pub magic: u32,
}

#[repr(C)]
#[derive(Default, Debug)]
pub struct ExecuteReq {
    pub magic: u64,
    pub env_flags: u64,  // env flags
    pub exec_flags: u64, // exec flags
    pub pid: u64,
    pub fault_call: u64,
    pub fault_nth: u64,
    pub syscall_timeout_ms: u64,
    pub program_timeout_ms: u64,
    pub slowdown_scale: u64,
    pub prog_size: u64,
}

#[repr(C)]
#[derive(Default, Debug)]
pub struct ExecuteReply {
    pub magic: u32,
    pub done: u32,
    pub status: u32,
}

#[repr(C)]
#[derive(Default, Debug)]
pub struct CallReply {
    pub index: u32, // call index in the program
    pub num: u32,   // syscall number (for cross-checking)
    pub errno: u32,
    pub flags: u32, // see CallFlags
    pub branch_size: u32,
    pub block_size: u32,
    pub comps_size: u32,
}
