#[cfg(feature = "amd64-linux")]
#[path = "linux/amd64.rs"]
#[rustfmt::skip]pub mod syscalls_inner;

#[cfg(feature = "386-linux")]
#[path = "linux/_386.rs"]
#[rustfmt::skip]pub mod syscalls_inner;

#[cfg(feature = "arm-linux")]
#[path = "linux/arm.rs"]
#[rustfmt::skip]pub mod syscalls_inner;

#[cfg(feature = "arm64-linux")]
#[path = "linux/arm64.rs"]
#[rustfmt::skip]pub mod syscalls_inner;

#[cfg(feature = "mips64le-linux")]
#[path = "linux/mips64le.rs"]
#[rustfmt::skip]pub mod syscalls_inner;

#[cfg(feature = "ppc64le-linux")]
#[path = "linux/ppc64le.rs"]
#[rustfmt::skip]pub mod syscalls_inner;

#[cfg(feature = "riscv64-linux")]
#[path = "linux/riscv64.rs"]
#[rustfmt::skip]pub mod syscalls_inner;

#[cfg(feature = "s390x-linux")]
#[path = "linux/s390x.rs"]
#[rustfmt::skip]pub mod syscalls_inner;

#[cfg(feature = "amd64-akaros")]
#[path = "akaros/amd64.rs"]
#[rustfmt::skip]pub mod syscalls_inner;

#[cfg(feature = "_386-freebsd")]
#[path = "freebsd/_386.rs"]
#[rustfmt::skip]pub mod syscalls_inner;

#[cfg(feature = "amd64-freebsd")]
#[path = "freebsd/amd64.rs"]
#[rustfmt::skip]pub mod syscalls_inner;

#[cfg(feature = "amd64-fuchsia")]
#[path = "fuchsia/amd64.rs"]
#[rustfmt::skip]pub mod syscalls_inner;

#[cfg(feature = "arm64-fuchsia")]
#[path = "fuchsia/arm64.rs"]
#[rustfmt::skip]pub mod syscalls_inner;

#[cfg(feature = "amd64-netbsd")]
#[path = "netbsd/amd64.rs"]
#[rustfmt::skip]pub mod syscalls_inner;

#[cfg(feature = "amd64-openbsd")]
#[path = "openbsd/amd64.rs"]
#[rustfmt::skip]pub mod syscalls_inner;

#[cfg(feature = "arm-trusty")]
#[path = "trusty/arm.rs"]
#[rustfmt::skip]pub mod syscalls_inner;

#[cfg(feature = "amd64-windows")]
#[path = "windows/amd64.rs"]
#[rustfmt::skip]pub mod syscalls_inner;

pub use syscalls_inner::*;
