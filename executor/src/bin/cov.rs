use executor::cover;
use executor::picoc::Picoc;

fn main() {
    let mut handle = cover::open();
    let mut pc = Picoc::default();

    let pc = handle.collect(|| {
        if !pc.execute(String::from(
            "\
    char buf[64];
    read(0,buf,64);
    ",
        )) {
            eprintln!("Failed");
        }
    });

    for p in pc {
        println!("{:#x}", p);
    }
}
