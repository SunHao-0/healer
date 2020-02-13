use crate::utils::cli::{App, Arg, OptVal};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Stdio;

lazy_static! {
    pub static ref SSH: App = {
        App::new("ssh")
            .arg(Arg::new_opt("-F", OptVal::normal("/dev/null")))
            .arg(Arg::new_opt(
                "-o",
                OptVal::normal("UserKnownHostsFile=/dev/null"),
            ))
            .arg(Arg::new_opt("-o", OptVal::normal("BatchMode=yes")))
            .arg(Arg::new_opt("-o", OptVal::normal("IdentitiesOnly=yes")))
            .arg(Arg::new_opt(
                "-o",
                OptVal::normal("StrictHostKeyChecking=no"),
            ))
            .arg(Arg::new_opt("-o", OptVal::normal("ConnectTimeout=10s")))
    };
    pub static ref SCP: App = {
        App::new("scp")
            .arg(Arg::new_opt("-F", OptVal::normal("/dev/null")))
            .arg(Arg::new_opt(
                "-o",
                OptVal::normal("UserKnownHostsFile=/dev/null"),
            ))
            .arg(Arg::new_opt("-o", OptVal::normal("BatchMode=yes")))
            .arg(Arg::new_opt("-o", OptVal::normal("IdentitiesOnly=yes")))
            .arg(Arg::new_opt(
                "-o",
                OptVal::normal("StrictHostKeyChecking=no"),
            ))
    };
}

#[derive(Debug, Deserialize)]
pub struct SshConfig {
    pub key_path: String,
    pub user: String,
}

pub fn ssh_run(key: &str, user: &str, addr: &str, port: u16, app: App) -> tokio::process::Child {
    let ssh = ssh_app(key, user, addr, port, app);

    let mut cmd = ssh.into_cmd();
    cmd.stdout(Stdio::piped())
         // .stderr(Stdio::piped())
         .stdin(Stdio::piped())
        .kill_on_drop(true);

    cmd.spawn()
        .unwrap_or_else(|e| panic!("Failed to spawn:{}", e))
}

pub fn ssh_app(key: &str, user: &str, addr: &str, port: u16, app: App) -> App {
    let mut ssh = SSH
        .clone()
        .arg(Arg::new_opt("-p", OptVal::normal(&port.to_string())))
        .arg(Arg::new_opt("-i", OptVal::normal(key)))
        .arg(Arg::Flag(format!("{}@{}", user, addr)))
        .arg(Arg::new_flag(&app.bin));
    for app_arg in app.iter_arg() {
        ssh = ssh.arg(Arg::Flag(app_arg));
    }
    ssh
}

pub async fn scp(key: &str, user: &str, addr: &str, port: u16, path: &PathBuf) {
    let scp = SCP
        .clone()
        .arg(Arg::new_opt("-P", OptVal::normal(&port.to_string())))
        .arg(Arg::new_opt("-i", OptVal::normal(key)))
        .arg(Arg::new_flag(path.as_path().to_str().unwrap()))
        .arg(Arg::Flag(format!("{}@{}:~/", user, addr)));

    let output = scp
        .into_cmd()
        .output()
        .await
        .unwrap_or_else(|e| panic!("Failed to spawn:{}", e));

    if !output.status.success() {
        panic!(String::from_utf8(output.stderr).unwrap())
    }
}
