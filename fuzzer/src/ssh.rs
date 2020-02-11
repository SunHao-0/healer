use crate::utils::cli::{App, Arg, OptVal};
use std::process::Stdio;

pub fn ssh_run(
    key: &str,
    user: &str,
    addr: &str,
    port: u16,
    app: Option<App>,
) -> tokio::process::Child {
    let ssh = App::new("ssh")
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
        .arg(Arg::new_opt("-p", OptVal::normal(&port.to_string())))
        .arg(Arg::new_opt("-i", OptVal::normal(key)))
        .arg(Arg::Flag(format!("{}@{}", user, addr)));
    let mut cmd = ssh.into_cmd();
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::piped())
        .kill_on_drop(true);
    if let Some(app) = app {
        cmd.arg(&app.bin);
        cmd.args(app.iter_arg());
    }
    cmd.spawn()
        .unwrap_or_else(|e| panic!("Failed to spawn:{}", e))
}
