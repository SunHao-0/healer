use executor::{exec, recv_prog, send};
use std::io::{stdin, stdout};

fn main() {
    let stdin = stdin();
    let stdout = stdout();

    let mut in_handle = stdin.lock();
    let mut out_handle = stdout.lock();

    loop {
        let p = recv_prog(&mut in_handle);
        let result = exec(&p);
        send(&result, &mut out_handle);
    }
}
