use crate::arch::{self, TARGET_ARCH};
use anyhow::Context;
use healer_vm::qemu::QemuConfig;
use regex::RegexSet;
use std::{
    env::current_dir,
    fs::{canonicalize, read_to_string},
    path::PathBuf,
    str::FromStr,
};
use syz_wrapper::{
    exec::{features::Features, ExecConfig},
    report::ReportConfig,
    repro::ReproConfig,
    sys::SysTarget,
};

#[derive(Clone)]
pub struct Config {
    pub os: String,
    pub relations: Option<PathBuf>,
    pub input: Option<PathBuf>,
    pub crash_whitelist: Option<PathBuf>,
    pub job: u64,
    pub syz_dir: PathBuf,
    pub output: PathBuf,
    pub skip_repro: bool,
    pub disable_relation_detect: bool,
    pub disabled_calls: Option<PathBuf>,
    pub features: Option<Features>,
    pub disable_fault_injection: bool,
    pub fault_injection_whitelist_path: Option<PathBuf>,
    pub fault_injection_regex: Option<RegexSet>,
    pub remote_exec: Option<PathBuf>,

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
            input: None,
            crash_whitelist: None,
            job: 1,
            syz_dir: current_dir().unwrap(),
            output: current_dir().unwrap().join("output"),
            skip_repro: false,
            disabled_calls: None,
            disable_relation_detect: false,
            features: None,
            disable_fault_injection: false,
            fault_injection_whitelist_path: None,
            fault_injection_regex: None,
            remote_exec: None,

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
        if let Some(i) = self.input.as_ref() {
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
        if self.output.exists() && !self.output.is_dir() {
            anyhow::bail!("'{}' not a directory", self.output.display());
        }
        if let Some(i) = self.disabled_calls.as_ref() {
            if !i.is_file() {
                anyhow::bail!("bad disabled calls file: {}", i.display());
            }
        }
        if self.disable_fault_injection && self.fault_injection_whitelist_path.is_some() {
            anyhow::bail!(
                "fault injection disabled: {}",
                self.fault_injection_whitelist_path
                    .as_ref()
                    .unwrap()
                    .display()
            );
        }
        if let Some(i) = self.fault_injection_whitelist_path.as_ref() {
            if !i.is_file() {
                anyhow::bail!("bad fault injection whitelist file: {}", i.display());
            }
            let call_re = read_to_string(i).unwrap();
            let re_str = call_re
                .lines()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>();
            if !re_str.is_empty() {
                match RegexSet::new(re_str) {
                    Ok(s) => self.fault_injection_regex = Some(s),
                    Err(e) => anyhow::bail!(
                        "bad re in fault injection whitelist file {}: {}",
                        i.display(),
                        e
                    ),
                }
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
        self.output = canonicalize(&self.output).unwrap();

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
        self.repro_config.work_dir = self.output.to_str().unwrap().to_string();
        self.repro_config.disk_img = self.qemu_config.disk_img.clone();
        self.repro_config.kernel_img = self.qemu_config.kernel_img.clone().unwrap();
        self.repro_config.ssh_key = self.qemu_config.ssh_key.clone();

        self.report_config.os = self.os.clone();
        self.report_config.arch = TARGET_ARCH.to_string();
        self.report_config.id = self.exec_config.as_ref().unwrap().pid;
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
        let target_dir = format!("{}_{}", self.os, TARGET_ARCH);
        self.syz_dir
            .join("bin")
            .join(target_dir)
            .join("syz-executor")
    }
}
