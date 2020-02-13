use crate::ssh::{scp, ssh_run};
use crate::utils::cli::App;
use crate::Config;
use bytes::BytesMut;
use core::prog::Prog;
use executor::{exec, ExecResult};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::process::Child;

// config for executor
#[derive(Debug, Deserialize)]
pub struct Executor {
    pub path: PathBuf,
}
#[allow(dead_code)]
pub struct ExecHandle {
    _handle: Child,
    stream: TcpStream,
}

pub async fn startup(conf: &Config, qemu_port: u16) -> ExecHandle {
    scp(
        &conf.ssh.key_path,
        &conf.ssh.user,
        "localhost",
        qemu_port,
        &conf.executor.path,
    )
    .await;

    //    let executor = App::new("./executor");
    let handle = ssh_run(
        &conf.ssh.key_path,
        &conf.ssh.user,
        "localhost",
        qemu_port,
        App::new("./executor"),
    );
    // let handle = process::spawn(ssh_executor, None);
    let stream = TcpStream::connect("localhost:7070").await.unwrap();
    stream.set_send_buffer_size(1024 * 1024 * 4).unwrap();
    stream.set_recv_buffer_size(1024 * 1024 * 4).unwrap();
    ExecHandle {
        _handle: handle,
        stream,
    }
}

impl ExecHandle {
    pub fn exec(&mut self, p: &Prog) -> ExecResult {
        exec(p)
    }

    #[allow(dead_code)]
    async fn send_prog(&mut self, p: &Prog) {
        let mut bin = bincode::serialize(p).unwrap();
        bin.shrink_to_fit();

        self.stream.write_u32(bin.len() as u32).await.unwrap();

        println!("Send len:{}", bin.len());
        self.stream.write_all(&bin).await.unwrap();

        println!("Send prog:{}", p.gid);
    }
    #[allow(dead_code)]
    async fn recv_result(&mut self) -> ExecResult {
        let len = self.stream.read_u32().await.unwrap();
        // let stdout = self.0.stdout.as_mut().unwrap();
        //        read_exact(&mut self.stream, &mut len).await;
        //        let len = u32::from_be_bytes(len);

        // let len = self.0.stdout.as_mut().unwrap().read_u16().await.unwrap();
        println!("Recv len:{}", len);

        let mut buf = BytesMut::with_capacity(len as usize);
        unsafe {
            buf.set_len(len as usize);
        }
        println!("buf len:{}", buf.len());
        self.stream.read_exact(&mut buf).await.unwrap();

        //        read_exact(&mut self.stream, &mut buf[..]).await;

        bincode::deserialize(&buf).unwrap()
    }
}
//
//async fn read_exact<T: AsyncRead>(src: &mut T, mut buf: &mut [u8]) {
//    use tokio::io::ErrorKind;
//    while !buf.is_empty() {
//        match src.read(buf).await {
//            Ok(n) => {
//                let tmp = buf;
//                buf = &mut tmp[n..];
//            }
//            Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
//            Err(e) => panic!(e),
//        }
//        println!("buf len:{}", buf.len());
//    }
//
//    if !buf.is_empty() {
//        panic!("failed to fill whole buffer")
//    }
//}