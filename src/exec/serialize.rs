use crate::model::{BinFmt, Call, Dir, Prog, ResValue, ResValueKind, Value, ValueKind};
use crate::targets::Target;

use bytes::BufMut;
use iota::iota;
use rustc_hash::FxHashMap;
use thiserror::Error;

iota! {
    const EXEC_INSTR_EOF : u64 = (u64::MAX) ^ (iota);
        , EXEC_INSTR_COPY_IN
        , EXEC_INSTR_COPY_OUT
}

iota! {
    const EXEC_ARG_CONST: u64 = iota;
        , EXEC_ARG_RESULT
        , EXEC_ARG_DATA
        , EXEC_ARG_CSUM
    const EXEC_ARG_DATA_READABLE : u64 = 1<<63;
}

iota! {
    const EXEC_ARG_CSUM_INET: u64 = iota;
}

iota! {
    const EXEC_ARG_CSUM_CHUNK_DATA: u64 = iota;
        , EXEC_ARG_CSYM_CHUNK_CONST
}

const EXEC_NO_COPYOUT: u64 = u64::MAX;

#[derive(Debug, Error)]
pub(crate) enum SerializeError {
    #[error("buffer two small to serialize the prog, provided size: {provided} bytes")]
    BufferTooSmall { provided: usize },
}

/// Serialize a prog into packed binary format.
pub(crate) fn serialize(t: &Target, p: &Prog, buf: &mut [u8]) -> Result<usize, SerializeError> {
    let mut ctx = ExecCtx {
        target: t,
        buf,
        res_args: FxHashMap::default(),
        copyout_seq: 0,
        eof: false,
    };
    for call in &p.calls {
        ctx.serialize_call(call);
    }
    ctx.write_u64(EXEC_INSTR_EOF);
    if !ctx.buf.has_remaining_mut() || ctx.eof {
        Err(SerializeError::BufferTooSmall {
            provided: buf.len(),
        })
    } else {
        Ok(ctx.buf.remaining_mut())
    }
}

struct ExecCtx<'a, 'b> {
    target: &'a Target,
    buf: &'b mut [u8],
    res_args: FxHashMap<*const ResValue, ArgInfo>,
    copyout_seq: u64,
    eof: bool,
}
#[derive(Clone)]
struct ArgInfo {
    addr: u64,
    idx: u64,
    ret: bool,
}

impl ArgInfo {
    pub fn with_ret(idx: u64) -> Self {
        Self {
            addr: 0,
            ret: true,
            idx,
        }
    }

    pub fn with_addr(addr: u64) -> Self {
        Self {
            addr,
            idx: 0,
            ret: false,
        }
    }
}

impl ExecCtx<'_, '_> {
    fn serialize_call(&mut self, c: &Call) {
        self.write_copyin(c);
        // ignore write csum for now.
        self.write_u64(c.meta.id as u64);
        if let Some(ret) = c.ret.as_ref() {
            if ret.res_rc().unwrap() != 0 {
                assert!(!self
                    .res_args
                    .contains_key(&(ret.res_val().unwrap() as *const _)));
                let idx = self.next_copyout_seq();
                self.res_args
                    .insert(ret.res_val().unwrap() as *const _, ArgInfo::with_ret(idx));
                self.write_u64(idx);
            } else {
                self.write_u64(EXEC_NO_COPYOUT);
            }
        } else {
            self.write_u64(EXEC_NO_COPYOUT);
        }
        self.write_u64(c.args.len() as u64);
        for val in &c.args {
            self.write_arg(val);
        }
        self.write_copyout(c);
    }

    fn write_copyin(&mut self, c: &Call) {
        iter_call_args(c, |arg, ctx| {
            if let Some(addr) = ctx.base {
                let mut val_addr = self.target.physical_addr(addr) + ctx.offset;
                val_addr -= arg.ty.unit_offset();
                if self.will_be_used(arg) {
                    self.res_args.insert(
                        arg.res_val().unwrap() as *const _,
                        ArgInfo::with_addr(val_addr),
                    );
                }
                if let ValueKind::Group { .. } | ValueKind::Union { .. } = &arg.kind {
                    return;
                }
                if arg.dir == Dir::Out
                    || arg.ty.is_pad()
                    || (arg.size() == 0 && !arg.ty.is_bitfield())
                {
                    return;
                }
                self.write_u64(EXEC_INSTR_COPY_IN);
                self.write_u64(val_addr);
                self.write_arg(arg);
            }
        })
    }

    fn write_copyout(&mut self, c: &Call) {
        iter_call_args(c, |arg, _| {
            if let Some(res_rc) = arg.res_rc() {
                if res_rc != 0 {
                    let res_val = arg.res_val().unwrap();
                    let mut info = self.res_args.get(&(res_val as *const _)).unwrap().clone();
                    if info.ret {
                        return;
                    }

                    info.idx = self.next_copyout_seq();
                    self.write_u64(EXEC_INSTR_COPY_OUT);
                    self.write_u64(info.idx);
                    self.write_u64(info.addr);
                    self.write_u64(arg.size());
                    self.res_args.insert(res_val as *const _, info);
                }
            }
        })
    }

    fn write_arg(&mut self, val: &Value) {
        match &val.kind {
            ValueKind::Scalar(_) => {
                let (scalar_val, pid_stride) = val.scalar_val();
                self.write_const_arg(
                    val.unit_sz(),
                    scalar_val,
                    val.ty.bf_offset(),
                    val.ty.bf_len(),
                    pid_stride,
                    val.ty.bin_fmt(),
                );
            }
            ValueKind::Res(res_val) => match &res_val.kind {
                ResValueKind::Own { .. } | ResValueKind::Null => {
                    let (scalar_val, _) = val.scalar_val();
                    self.write_const_arg(val.size(), scalar_val, 0, 0, 0, val.ty.bin_fmt());
                }
                ResValueKind::Ref { src } => {
                    self.write_u64(EXEC_ARG_RESULT);
                    let meta = val.size() | (val.ty.bin_fmt() as u64) << 8;
                    self.write_u64(meta);
                    self.write_u64(self.res_args[&(&**src as *const _)].idx);
                    self.write_u64(src.op_div);
                    self.write_u64(src.op_add);
                    self.write_u64(res_val.val);
                }
            },
            ValueKind::Ptr { addr, .. } | ValueKind::Vma { addr, .. } => {
                self.write_const_arg(
                    val.size(),
                    self.target.physical_addr(*addr),
                    0,
                    0,
                    0,
                    BinFmt::Native,
                );
            }
            ValueKind::Bytes(bs) => {
                if bs.is_empty() {
                    return;
                }
                self.write_u64(EXEC_ARG_DATA);
                let mut flags = bs.len() as u64;
                if val.ty.is_readable_date_type() {
                    flags |= EXEC_ARG_DATA_READABLE;
                }
                self.write_u64(flags);
                let pad = 8 - bs.len() % 8;
                self.write_slice(bs);
                if pad != 8 {
                    static PAD: [u8; 8] = [0; 8];
                    self.write_slice(&PAD[0..pad]);
                }
            }
            ValueKind::Union { val, .. } => self.write_arg(val),
            _ => unreachable!(),
        }
    }

    fn write_const_arg(
        &mut self,
        sz: u64,
        val: u64,
        bf_offset: u64,
        bf_len: u64,
        pid_stride: u64,
        bf: BinFmt,
    ) {
        self.write_u64(EXEC_ARG_CONST);
        let meta = sz | (bf as u64) << 8 | bf_offset << 16 | bf_len << 24 | pid_stride << 32;
        self.write_u64(meta);
        self.write_u64(val);
    }

    fn write_u64(&mut self, val: u64) {
        if self.buf.len() >= 8 {
            if self.target.le_endian {
                self.buf.put_u64_le(val);
            } else {
                self.buf.put_u64(val);
            }
        } else {
            self.eof = true;
        }
    }

    fn write_slice<T: AsRef<[u8]>>(&mut self, slice: T) {
        if self.buf.len() >= slice.as_ref().len() {
            self.buf.put_slice(slice.as_ref());
        } else {
            self.eof = true;
        }
    }

    fn will_be_used(&self, arg: &Value) -> bool {
        // ignore csum type for now
        if let Some(res_rc) = arg.res_rc() {
            res_rc != 0
        } else {
            false
        }
    }

    fn next_copyout_seq(&mut self) -> u64 {
        let tmp = self.copyout_seq;
        self.copyout_seq += 1;
        tmp
    }
}
#[derive(Default, Clone)]
struct ArgCtx {
    base: Option<u64>,
    offset: u64,
}

fn iter_call_args<F>(c: &Call, mut f: F)
where
    F: FnMut(&Value, &ArgCtx),
{
    let mut ctx = ArgCtx::default();
    for val in &c.args {
        iter_arg(val, &mut ctx, &mut f)
    }
}

fn iter_arg<F>(val: &Value, ctx: &mut ArgCtx, f: &mut F)
where
    F: FnMut(&Value, &ArgCtx),
{
    let ctx_pre = ctx.clone();
    f(val, ctx);
    match &val.kind {
        ValueKind::Group(vals) => {
            for val in vals {
                iter_arg(val, ctx, f);
                ctx.offset += val.size();
            }
        }
        ValueKind::Union { val, .. } => iter_arg(val, ctx, f),
        ValueKind::Ptr { addr, pointee } => {
            if let Some(pointee) = pointee.as_ref() {
                ctx.base = Some(*addr);
                ctx.offset = 0;
                iter_arg(pointee, ctx, f);
            }
        }
        _ => {}
    }
    *ctx = ctx_pre;
}
