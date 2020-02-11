use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use core::prog::Prog;
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::io::prelude::*;

#[derive(Serialize, Deserialize)]
pub struct ExecResult(Vec<Vec<u32>>);

#[derive(Serialize, Deserialize)]
pub struct FeedBack;

// Just a stub
pub fn exec(p: &Prog) -> ExecResult {
    let mut result = Vec::with_capacity(p.len());
    let mut rng = thread_rng();

    for _ in 0..p.len() {
        let len = rng.gen_range(128, 1024);
        let mut pcs = (0..len).map(|_| rng.gen::<u32>()).collect::<Vec<_>>();
        pcs.shrink_to_fit();
        result.push(pcs);
    }
    ExecResult(result)
}

pub fn recv_prog<T: Read>(src: &mut T) -> Prog {
    let len = src.read_u16::<BigEndian>().unwrap();
    let mut buf = Vec::with_capacity(len as usize);
    src.read_exact(&mut buf).unwrap();
    bincode::deserialize(&buf).unwrap()
}

pub fn recv_exec_result<T: Read>(src: &mut T) -> ExecResult {
    let len = src.read_u16::<BigEndian>().unwrap();
    let mut buf = Vec::with_capacity(len as usize);
    src.read_exact(&mut buf).unwrap();
    bincode::deserialize(&buf).unwrap()
}
//
//pub fn recv<'a,T:Deserialize<'a> + 'a, R:Read>(src:&mut R) -> T{
//    let len = src.read_u16::<BigEndian>().unwrap();
//    let mut buf = Vec::with_capacity(len as usize);
//    src.read_exact(&mut buf).unwrap();
//
//    bincode::deserialize(&buf).unwrap()
//}

pub fn send<T: Serialize, W: Write>(data: &T, out: &mut W) {
    let mut bin = bincode::serialize(data).unwrap();
    bin.shrink_to_fit();

    out.write_u16::<BigEndian>(bin.len() as u16).unwrap();
    out.write_all(&bin).unwrap();
}
