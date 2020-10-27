pub mod amd64;
pub mod arm64;
pub mod arm;
pub mod _386;
pub mod mips64le;
pub mod ppc64le;
pub mod riscv64;
pub mod s390x;

pub use amd64::syscalls as syscalls_amd64;
pub use arm64::syscalls as syscalls_arm64;