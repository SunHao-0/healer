use std::{path::PathBuf, process::Stdio};

use crate::utils::ssh;

use super::ManageVm;
use thiserror::Error;
#[derive(Debug, Clone)]
pub struct NullHandleConfig {
    pub ssh_ip: String,
    pub ssh_port: u16,
    pub ssh_user: String,
    pub ssh_key: PathBuf,
}

impl NullHandleConfig {
    pub fn check(&self) -> Result<(), String> {
        let mut handle = NullHandle::with_config(self.clone());
        if !handle.is_alive() {
            Err("machine not alive".to_string())
        } else {
            Ok(())
        }
    }
}

pub struct NullHandle {
    ssh_ip: String,
    ssh_port: u16,
    ssh_user: String,
    ssh_key: PathBuf,
    alive: bool,
}

impl NullHandle {
    pub fn with_config(config: NullHandleConfig) -> Self {
        Self {
            ssh_ip: config.ssh_ip,
            ssh_port: config.ssh_port,
            ssh_user: config.ssh_user,
            ssh_key: config.ssh_key,
            alive: true,
        }
    }
}

impl ManageVm for NullHandle {
    type Error = NoneHandleError;

    fn boot(&mut self) -> Result<(), Self::Error> {
        if self.is_alive() {
            Ok(())
        } else {
            Err(NoneHandleError::Crashed)
        }
    }

    fn addr(&self) -> Option<(String, u16)> {
        if self.alive {
            Some((self.ssh_ip.clone(), self.ssh_port))
        } else {
            None
        }
    }

    fn ssh(&self) -> Option<(std::path::PathBuf, String)> {
        if self.alive {
            Some((self.ssh_key.clone(), self.ssh_user.clone()))
        } else {
            None
        }
    }

    fn is_alive(&mut self) -> bool {
        if !self.alive {
            return false;
        }

        let (qemu_ip, qemu_port) = self.addr().unwrap();
        let mut ssh_cmd = ssh::ssh_basic_cmd(
            &qemu_ip,
            qemu_port,
            &self.ssh_key.to_str().unwrap().to_string(),
            &self.ssh_user,
        );
        let status = ssh_cmd
            .arg("pwd")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap(); // ignore this error
        self.alive = status.success();
        self.alive
    }

    fn collect_crash_log(&mut self) -> Vec<u8> {
        Vec::new()
    }

    fn reset(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum NoneHandleError {
    #[error("machine already crashed")]
    Crashed,
}
