use std::path::Path;
use std::process::Command;

#[derive(Debug)]
pub struct SshConf {
    pub ssh_key: Box<Path>,
    pub ssh_user: Option<String>, // root for default
}

pub fn build_basic_cmd<T: AsRef<str>>(
    ip: T,
    port: u16,
    key: T, // use key instead of password
    user: T,
) -> Command {
    let mut ssh_cmd = Command::new("ssh");
    ssh_cmd
        .args(vec!["-F", "/dev/null"]) // posix only
        .args(vec!["-o", "BatchMode=yes"])
        .args(vec!["-o", "IdentitiesOnly=yes"])
        .args(vec!["-o", "StrictHostKeyChecking=no"])
        .args(vec!["-o", "UserKnownHostsFile=/dev/null"])
        .args(vec!["-o", "ConnectTimeout=10s"])
        .args(vec!["-i", key.as_ref()])
        .args(vec!["-p", &format!("{}", port)])
        .arg(format!("{}@{}", user.as_ref(), ip.as_ref()));
    ssh_cmd
}
