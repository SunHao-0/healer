use executor::picoc::Picoc;

fn main() {
    let mut handle = cover::open();
    let mut pc = Picoc::default();

    let prog_part1: String = "
                char buf[64];
                uint32_t a = 0;
                for(;a<64;a++){
                    buf[a] = a;
                }
                char *s = \"hello, world\";
                "
    .into();
    let prog_part2: String = "
        _int sum = 0;
        for(int i = 0;i<64;i++){
            sum += buf[i];
        }
        printf(\"%d\n\",sum);
        printf(\"%s\n\",s);
    "
    .into();

    let covs = handle.collect(|| {
        pc.execute(prog_part1.clone());
        pc.execute(prog_part2.clone());
    });

    for p in covs {
        println!("{:#x}", p);
    }
}
