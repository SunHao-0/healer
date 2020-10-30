#[cfg(feature = "amd64-linux")]
#[path = "syscalls/linux/amd64.rs"]
pub mod syscalls;

#[cfg(feature = "386-linux")]
#[path = "syscalls/linux/_386.rs"]
pub mod syscalls;

#[cfg(feature = "arm-linux")]
#[path = "syscalls/linux/arm.rs"]
pub mod syscalls;

#[cfg(feature = "arm64-linux")]
#[path = "syscalls/linux/arm64.rs"]
pub mod syscalls;

#[cfg(feature = "mips64le-linux")]
#[path = "syscalls/linux/mips64le.rs"]
pub mod syscalls;

#[cfg(feature = "ppc64le-linux")]
#[path = "syscalls/linux/ppc64le.rs"]
pub mod syscalls;

#[cfg(feature = "riscv64-linux")]
#[path = "syscalls/linux/riscv64.rs"]
pub mod syscalls;

#[cfg(feature = "s390x-linux")]
#[path = "syscalls/linux/s390x.rs"]
pub mod syscalls;

#[cfg(feature = "amd64-akaros")]
#[path = "syscalls/akaros/amd64.rs"]
pub mod syscalls;

#[cfg(feature = "_386-freebsd")]
#[path = "syscalls/freebsd/_386.rs"]
pub mod syscalls;

#[cfg(feature = "amd64-freebsd")]
#[path = "syscalls/freebsd/amd64.rs"]
pub mod syscalls;

#[cfg(feature = "amd64-fuchsia")]
#[path = "syscalls/fuchsia/amd64.rs"]
pub mod syscalls;

#[cfg(feature = "arm64-fuchsia")]
#[path = "syscalls/fuchsia/arm64.rs"]
pub mod syscalls;

#[cfg(feature = "amd64-netbsd")]
#[path = "syscalls/netbsd/amd64.rs"]
pub mod syscalls;

#[cfg(feature = "amd64-openbsd")]
#[path = "syscalls/openbsd/amd64.rs"]
pub mod syscalls;

#[cfg(feature = "arm-trusty")]
#[path = "syscalls/trusty/arm.rs"]
pub mod syscalls;

#[cfg(feature = "amd64-windows")]
#[path = "syscalls/windows/amd64.rs"]
pub mod syscalls;
