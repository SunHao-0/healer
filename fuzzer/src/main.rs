use fuzzer::qemu;
use std::env;

#[tokio::main]
async fn main() {
    let image = env::args().nth(1).unwrap();
    let kernel = env::args().nth(2).unwrap();

    let cfg = qemu::Cfg {
        target: "linux/amd64".to_string(),
        mem_size: 2048,
        cpu_num: 2,
        image,
        kernel,
        ssh_key_path: "stretch.id_rsa".to_string(),
        ssh_user: "root".to_string(),
    };

    let handle = qemu::boot(&cfg).await;
    println!("{:?}", handle);
}
