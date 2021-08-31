use bytes::Buf;
use core::slice;
use std::{
    io::{Read, Write},
    mem,
};

pub fn read_u32(buf: &mut &[u8]) -> Option<u32> {
    if buf.remaining() >= 4 {
        let val = if cfg!(target_endian = "little") {
            buf.get_u32_le()
        } else {
            buf.get_u32()
        };
        Some(val)
    } else {
        None
    }
}

pub fn read_u32_slice<'a>(buf: &mut &'a [u8], len: usize) -> Option<&'a [u32]> {
    let l = len * mem::size_of::<u32>();
    if l <= buf.len() {
        let ret = unsafe { slice::from_raw_parts(buf.as_ptr() as *const u32, len) };
        buf.advance(l);
        Some(ret)
    } else {
        None
    }
}

pub fn read<'a, T: Sized>(buf: &mut &'a [u8]) -> Option<&'a T> {
    let sz = mem::size_of::<T>();
    if buf.len() >= sz {
        let buf0 = &buf[0..sz];
        let v = cast_from(buf0);
        buf.advance(sz);
        Some(v)
    } else {
        None
    }
}

pub fn read_exact<T: Default + Sized, R: Read>(mut r: R) -> Result<T, std::io::Error> {
    let mut v = T::default();
    let data = cast_to_mut(&mut v);
    r.read_exact(data)?;
    Ok(v)
}

pub fn write_all<T: Sized, W: Write>(mut w: W, v: &T) -> Result<(), std::io::Error> {
    let data = cast_to(v);
    w.write_all(data)
}

pub fn cast_to<T: Sized>(v: &T) -> &[u8] {
    let ptr = (v as *const T).cast::<u8>();
    let len = mem::size_of::<T>();
    unsafe { slice::from_raw_parts(ptr, len) }
}

pub fn cast_to_mut<T: Sized>(v: &mut T) -> &mut [u8] {
    let ptr = (v as *mut T).cast::<u8>();
    let len = mem::size_of::<T>();
    unsafe { slice::from_raw_parts_mut(ptr, len) }
}

pub fn cast_from<T: Sized>(v: &[u8]) -> &T {
    assert_eq!(v.len(), mem::size_of::<T>());
    let ptr = v.as_ptr() as *const T;
    unsafe { ptr.as_ref().unwrap() }
}
