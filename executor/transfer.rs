//! A implementation of very sample object transfer protocal.

use crate::ExecResult;
use bytes::BytesMut;
use core::prog::Prog;
use serde::{Deserialize, Serialize};
use std::io;
use std::io::{Read, Write};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Header {
    pub len: u32,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Io:{0}")]
    Io(#[from] io::Error),
    #[error("Serialize: {0}")]
    Serialize(#[from] bincode::Error),
}

pub fn recv_prog<S: Read>(src: &mut S) -> Result<Prog, Error> {
    let header = Header::default();
    let headler_len = bincode::serialized_size(&header)? as usize;

    let mut header_buf = BytesMut::with_capacity(headler_len);
    assert!(header_buf.capacity() >= headler_len);
    unsafe {
        header_buf.set_len(headler_len);
    }
    src.read_exact(&mut header_buf)?;
    let header: Header = bincode::deserialize(&header_buf)?;

    let body_len = header.len as usize;
    let mut body_buf = BytesMut::with_capacity(body_len);
    unsafe {
        body_buf.set_len(body_len);
    }
    src.read_exact(&mut body_buf)?;

    bincode::deserialize(&body_buf).map_err(|e| e.into())
}

pub fn send<T: Serialize, S: Write>(v: &T, out: &mut S) -> Result<(), Error> {
    let len = bincode::serialized_size(v)? as u32;
    let header = Header { len };

    let header = bincode::serialize(&header)?;
    let body = bincode::serialize(v)?;

    out.write_all(&header)?;
    out.write_all(&body)?;

    Ok(())
}

pub async fn async_send<T: Serialize, S: AsyncWrite + Unpin>(
    p: &T,
    out: &mut S,
) -> Result<(), Error> {
    let len = bincode::serialized_size(p)? as u32;
    let header = Header { len };
    let header = bincode::serialize(&header)?;
    let body = bincode::serialize(p)?;

    out.write_all(&header).await?;
    out.write_all(&body).await?;
    Ok(())
}

pub async fn async_recv_result<T: AsyncRead + Unpin>(src: &mut T) -> Result<ExecResult, Error> {
    let header = Header::default();
    let headler_len = bincode::serialized_size(&header)? as usize;
    let mut header_buf = BytesMut::with_capacity(headler_len);
    assert!(header_buf.capacity() >= headler_len);
    unsafe {
        header_buf.set_len(headler_len);
    }
    src.read_exact(&mut header_buf).await?;
    let header: Header = bincode::deserialize(&header_buf)?;

    let body_len = header.len as usize;
    let mut body_buf = BytesMut::with_capacity(body_len);
    unsafe {
        body_buf.set_len(body_len);
    }
    src.read_exact(&mut body_buf).await?;

    bincode::deserialize(&body_buf).map_err(|e| e.into())
}
