use crate::arch::{self, TARGET_ARCH};
use anyhow::Context;
use healer_vm::qemu::QemuConfig;
use std::{env::current_dir, fs::canonicalize, path::PathBuf, str::FromStr};
use syz_wrapper::{exec::ExecConfig, report::ReportConfig, repro::ReproConfig, sys::SysTarget};

#[derive(Clone)]
pub struct Config {
    pub os: String,
    pub relations: Option<PathBuf>,
    pub input_prog: Option<PathBuf>,
    pub crash_whitelist: Option<PathBuf>,
    pub job: usize,
    pub syz_dir: PathBuf,
    pub output_dir: PathBuf,
    pub skip_repro: bool,
    pub disable_relation_detect: bool,
    pub disabled_calls: Option<PathBuf>,

    pub qemu_config: QemuConfig,
    pub repro_config: ReproConfig,
    pub report_config: ReportConfig,
    pub exec_config: Option<ExecConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            os: "linux".to_string(),
            relations: None,
            input_prog: None,
            crash_whitelist: None,
            job: 1,
            syz_dir: current_dir().unwrap(),
            output_dir: current_dir().unwrap().join("output"),
            skip_repro: false,
            disabled_calls: None,
            disable_relation_detect: false,

            qemu_config: QemuConfig::default(),
            exec_config: None,
            repro_config: ReproConfig::default(),
            report_config: ReportConfig::default(),
        }
    }
}

unsafe impl Send for Config {} // do not send config while holding shm.
unsafe impl Sync for Config {}

impl Config {
    pub fn check(&mut self) -> anyhow::Result<()> {
        let target_name = format!("{}/{}", self.os, arch::TARGET_ARCH);
        if SysTarget::from_str(&target_name).is_err() {
            anyhow::bail!("unsupported target: {}", target_name);
        }
        if let Some(r) = self.relations.as_ref() {
            if !r.is_file() {
                anyhow::bail!("bad relations file: {}", r.display());
            }
        }
        if let Some(i) = self.input_prog.as_ref() {
            if !i.is_dir() {
                anyhow::bail!("bad input progs dir: {}", i.display());
            }
        }
        if let Some(r) = self.crash_whitelist.as_ref() {
            if !r.is_file() {
                anyhow::bail!("bad crash whitelist file: {}", r.display());
            }
        }
        if !self.syz_dir.is_dir() {
            anyhow::bail!("bad syz-dir: {}", self.syz_dir.display());
        }
        let bin_dir = self.syz_dir.join("bin");
        if !bin_dir.is_dir() {
            anyhow::bail!("'bin' dir not exists in syz-dir: {}", bin_dir.display());
        }
        let target_dir = format!("{}_{}", self.os, TARGET_ARCH);
        let target_bin_dir = bin_dir.join(target_dir);
        if !target_bin_dir.is_dir() {
            anyhow::bail!("{} not exists", target_bin_dir.display());
        }
        let syz_executor = target_bin_dir.join("syz-executor");
        if !syz_executor.exists() {
            anyhow::bail!("{} not exists", syz_executor.display());
        }
        if self.output_dir.is_dir() {
            anyhow::bail!(
                "output dir ({}) already existed, cleanup first",
                self.output_dir.display()
            );
        }
        if let Some(i) = self.disabled_calls.as_ref() {
            if !i.is_file() {
                anyhow::bail!("bad disabled calls file: {}", i.display());
            }
        }

        self.qemu_config.check().context("qemu config error")?;
        self.repro_config
            .check()
            .map_err(|e| anyhow::anyhow!(e))
            .context("repro config error")?;
        self.report_config
            .check()
            .map_err(|e| anyhow::anyhow!(e))
            .context("report config error")?;
        Ok(())
    }

    pub fn fixup(&mut self) {
        let target_name = format!("{}/{}", self.os, arch::TARGET_ARCH);
        self.syz_dir = canonicalize(&self.syz_dir).unwrap();
        self.output_dir = canonicalize(&self.output_dir).unwrap();

        self.qemu_config.target = target_name;
        if let Some(kernel_img) = self.qemu_config.kernel_img.as_mut() {
            let path = canonicalize(&kernel_img).unwrap();
            *kernel_img = path.to_str().unwrap().to_string();
        }
        let path = canonicalize(&self.qemu_config.disk_img).unwrap();
        self.qemu_config.disk_img = path.to_str().unwrap().to_string();
        let path = canonicalize(&self.qemu_config.ssh_key).unwrap();
        self.qemu_config.ssh_key = path.to_str().unwrap().to_string();

        self.repro_config.id = self.exec_config.as_ref().unwrap().pid;
        self.repro_config.target = self.qemu_config.target.clone();
        self.repro_config.syz_dir = self.syz_dir.to_str().unwrap().to_string();
        self.repro_config.work_dir = self.output_dir.to_str().unwrap().to_string();
        self.repro_config.disk_img = self.qemu_config.disk_img.clone();
        self.repro_config.kernel_img = self.qemu_config.kernel_img.clone().unwrap();
        self.repro_config.ssh_key = self.qemu_config.ssh_key.clone();

        self.report_config.arch = TARGET_ARCH.to_string();
        self.repro_config.id = self.exec_config.as_ref().unwrap().pid;
        self.report_config.syz_dir = self.syz_dir.to_str().unwrap().to_string();
        if let Some(kernel_obj) = self.report_config.kernel_obj_dir.as_mut() {
            let path = canonicalize(&kernel_obj).unwrap();
            *kernel_obj = path.to_str().unwrap().to_string();
        }
        if let Some(kernel_src) = self.report_config.kernel_src_dir.as_mut() {
            let path = canonicalize(&kernel_src).unwrap();
            *kernel_src = path.to_str().unwrap().to_string();
        }
    }

    pub fn syz_executor(&self) -> PathBuf {
        self.syz_dir.join("bin").join("syz-executor")
    }
}
