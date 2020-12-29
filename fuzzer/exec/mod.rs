/// Communication with syz-executor.
// pub mod comm;
pub mod qemu;
/// Prog Serialization.
pub mod serialize;
pub mod ssh;

// use hlang::ast::Prog;
// use iota::iota;
// use std::path::{Path, PathBuf};

// /// Env flags to executor.
// pub type EnvFlags = u64;

// iota! {
//     pub const FLAG_DEBUG: EnvFlags = 1 << (iota);             // debug output from executor
//     , FLAG_SIGNAL                                    // collect feedback signals (coverage)
//     , FLAG_SANDBOX_SETUID                            // impersonate nobody user
//     , FLAG_SANDBOX_NAMESPACE                         // use namespaces for sandboxing
//     , FLAG_SANDBOX_ANDROID                           // use Android sandboxing for the untrusted_app domain
//     , FLAG_EXTRA_COVER                               // collect extra coverage
//     , FLAG_ENABLE_TUN                                // setup and use /dev/tun for packet injection
//     , FLAG_ENABLE_NETDEV                             // setup more network devices for testing
//     , FLAG_ENABLE_NETRESET                           // reset network namespace between programs
//     , FLAG_ENABLE_CGROUPS                            // setup cgroups for testing
//     , FLAG_ENABLE_CLOSEFDS                          // close fds after each program
//     , FLAG_ENABLE_DEVLINKPCI                         // setup devlink PCI device
//     , FLAG_ENABLE_VHCI_INJECTION                     // setup and use /dev/vhci for hci packet injection
//     , FLAG_ENABLE_WIFI                               // setup and use mac80211_hwsim for wifi emulation
// }

// type ExecFlags = u64;

// iota! {
//     const FLAG_COLLECT_COVER : ExecFlags = 1 << (iota);       // collect coverage
//     , FLAG_DEDUP_COVER                                 // deduplicate coverage in executor
//     , FLAG_INJECT_FAULT                                // inject a fault in this execution (see ExecOpts)
//     , FLAG_COLLECT_COMPS                               // collect KCOV comparisons
//     , FLAG_THREADED                                    // use multiple threads to mitigate blocked syscalls
//     , FLAG_COLLIDE                                     // collide syscalls to provoke data races
//     , FLAG_ENABLE_COVERAGE_FILTER                      // setup and use bitmap to do coverage filter
// }

// pub enum ExecResult {
//     Normal(Vec<CallExecInfo>),
//     Crash(String), // TODO use structural crash information.
// }

// type CallFlags = u32;

// iota! {
//     const CALL_EXECUTED : CallFlags = 1 << (iota); // was started at all
//     , CALL_FINISHED                                // finished executing (rather than blocked forever)
//     , CALL_BLOCKED                                 // finished but blocked during execution
//     , CALL_FAULT_INJECTED                          // fault was injected into this call
// }

// pub type Branch = u32;

// pub type Block = u32;

// pub struct CallExecInfo {
//     flags: CallFlags,
//     branches: Vec<Branch>,
//     blocks: Vec<Block>,
//     errno: i32,
// }

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
