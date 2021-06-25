//! VM manager.

use std::path::PathBuf;

pub mod null;
pub mod qemu;

pub const HEALER_VM_PID: &str = "GRAFTER_VM_PID";

pub trait ManageVm {
    type Error: std::error::Error + 'static;

    fn boot(&mut self) -> Result<(), Self::Error>;
    fn addr(&self) -> Option<(String, u16)>;
    fn ssh(&self) -> Option<(PathBuf, String)>;
    fn is_alive(&mut self) -> bool;
    fn collect_crash_log(&mut self) -> Vec<u8>;
    fn reset(&mut self) -> Result<(), Self::Error>;
}
