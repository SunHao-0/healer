use hl_fuzzer::target::*;

pub fn main() {
    // Parse command line arguments, validate thems
    // Extract env vars
    // Maybe add some performance operations, such as bind cpu
    // start the fuzz instance.
    let target = Target::new();
    let mut count = 0;
    for res_tys in target.res_tys.iter() {
        let desc = res_tys.res_desc().unwrap();
        count += desc.ctors.len();
        count += desc.consumers.len();
        println!(
            "Res: {}.\n\tCTORS: {:?}\n\tCONS: {:?}",
            res_tys.name,
            desc.ctors.len(),
            desc.consumers.len()
        );
    }

    println!("{}", target.res_tys.len());
    println!("{}", count);
    println!("{}", target.syscalls.len());
}
