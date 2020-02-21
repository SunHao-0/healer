use executor::cover;
use executor::picoc::Picoc;

fn main() {
    let mut handle = cover::open();
    let mut pc = Picoc::default();
    let prog = "char buf[64];
                read(0,buf,64);"
        .into();

    let pc = handle.collect(|| {
        if !pc.execute(prog) {
            eprintln!("Failed");
        }
    });

    for p in pc {
        println!("{:#x}", p);
    }
}
