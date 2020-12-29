use iota::iota;

/// Communication with syz-executor.
pub mod comm;
pub mod qemu;
/// Prog Serialization.
pub mod serialize;
pub mod ssh;
pub mod syz;

pub enum ExecResult {
    Normal(Vec<CallExecInfo>),
    Crash(String), // TODO use structural crash information.
}

type CallFlags = u32;

iota! {
    const CALL_EXECUTED : CallFlags = 1 << (iota); // was started at all
    , CALL_FINISHED                                // finished executing (rather than blocked forever)
    , CALL_BLOCKED                                 // finished but blocked during execution
    , CALL_FAULT_INJECTED                          // fault was injected into this call
}

pub type Branch = u32;

pub type Block = u32;

pub struct CallExecInfo {
    flags: CallFlags,
    branches: Vec<Branch>,
    blocks: Vec<Block>,
    errno: i32,
}

// pub struct ExecHandle {
//     /// Path to executor executable file.
//     exec_path: Box<Path>,
//     use_shm: bool,
//     use_forksrv: bool,
//     exec_env: EnvFlags,
// }

// impl ExecHandle {
//     pub fn new<T: Into<PathBuf>>(p: T) -> Self {
//         Self {
//             exec_path: p.into().into_boxed_path(),
//             use_shm: true, // use shm and fork server by default for now.
//             use_forksrv: true,
//             exec_env: FLAG_SIGNAL,
//         }
//     }

//     pub fn spwan_on_qemu() -> Result<(), ()> {
//         todo!()
//     }

//     pub fn exec(&mut self, p: &Prog) -> Result<ExecResult, ()> {
//         todo!()
//     }
// }

// struct HandleInner {
//     exec_handle: (),
//     exec_in: (),
//     exec_out: (),
//     exec_err: (),
//     qemu_handle: (),
//     qemu_err: (),
//     shm_in: (),
//     shm_out: (),
// }
