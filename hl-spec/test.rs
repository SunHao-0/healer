use hl_spec::*;

fn main() {
    let ident_tests = vec!["@il ident", "_identifier"];
    for ident in &ident_tests {
        let ret = parse_ident_test(ident);
        if let Err(e) = ret {
            eprintln!("{}", e);
        } else {
            println!("{:?}", ret)
        }
    }

    let integers = vec![
        "-", "-0", "-0x", "0x", "0xf", "-0oF", "-0o1", "00012", "-123",
    ];
    for i in &integers {
        let ret = parse_integer_test::<i32>(i);
        if let Err(e) = ret {
            eprintln!("{}", e)
        } else {
            println!("{:?}", ret)
        }
    }
}
