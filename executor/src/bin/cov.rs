use executor::cover;
use nix::unistd::{read,write};

fn main() {
    let mut handle = cover::open();
    let mut buf = [0; 64];

    let c1 = handle.collect(|| {
        read(0, &mut buf);
        write(0,&buf);
    });

    for i in c1 {
        println!("{:#x?}", i);
    }
}
