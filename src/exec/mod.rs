use crate::exec::syz::SyzExecConfig;
use crate::exec::syz::{SyzExecHandle, INPUT_SHM_SZ};
use crate::targets::Target;
use crate::vm::qemu::{QemuConfig, QemuHandle, QemuHandleError};
use rustc_hash::FxHashSet;
use shared_memory::{Shmem, ShmemConf, ShmemError};
use thiserror::Error;

/// Prog Serialization.
pub mod serialize;
/// Syz-executor handling.
pub mod syz;

#[derive(Debug, Error)]
pub enum SpawnError {
    #[error("failed to spawn executor: {0}")]
    Syz(#[from] syz::SyzSpawnError),
    #[error("failed to use shm: {0}")]
    Shm(#[from] ShmemError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Boot qemu with 'qemu_conf' and 'ssh_conf', then spawn inner executor in it.
pub fn spawn_syz_in_qemu(
    conf: SyzExecConfig,
    mut qemu_conf: QemuConfig,
    pid: u64,
) -> Result<SyzExecHandle<QemuHandleError>, SpawnError> {
    const IN_MEM_SIZE: usize = 4 << 20;
    const OUT_MEM_SIZE: usize = 16 << 20;

    let target = Target::new(&qemu_conf.target, &FxHashSet::default()).unwrap();
    let (mut in_shm, mut out_shm) = (None, None);
    let (mut in_mem, mut out_mem) = (None, None);

    if target.syz_exec_use_shm {
        let in_shm_id = format!("healer-in_shm-{}-{}", pid, std::process::id());
        let out_shm_id = format!("healer-out_shm_{}-{}", pid, std::process::id());
        in_shm = Some(shm(&in_shm_id, IN_MEM_SIZE)?);
        out_shm = Some(shm(&out_shm_id, OUT_MEM_SIZE)?);
        qemu_conf.add_shm(&in_shm_id, INPUT_SHM_SZ);
        qemu_conf.add_shm(&out_shm_id, OUT_MEM_SIZE);
    } else {
        in_mem = Some(boxed_buf(IN_MEM_SIZE));
        out_mem = Some(boxed_buf(OUT_MEM_SIZE));
    }

    let qemu_handle = QemuHandle::with_config(qemu_conf);

    let mut handle = SyzExecHandle::new(qemu_handle, conf);
    handle.in_mem = in_mem;
    handle.out_mem = out_mem;
    handle.in_shm = in_shm;
    handle.out_shm = out_shm;
    handle.spawn_syz(false)?;

    Ok(handle)
}

fn shm<T: AsRef<str>>(id: T, sz: usize) -> Result<Shmem, ShmemError> {
    let id = id.as_ref();
    match ShmemConf::new().os_id(id).size(sz).create() {
        Ok(mut shm) => {
            shm.set_owner(true);
            Ok(shm)
        }
        Err(ShmemError::MappingIdExists) => ShmemConf::new().os_id(id).size(sz).open(),
        Err(e) => Err(e),
    }
}

fn boxed_buf(sz: usize) -> Box<[u8]> {
    let mut buf: Vec<u8> = Vec::with_capacity(sz);
    unsafe {
        buf.set_len(sz);
    }
    for i in &mut buf {
        *i = 0;
    }
    buf.into_boxed_slice()
}
