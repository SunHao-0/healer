use crate::HashMap;
use bytes::BufMut;
use healer_core::{
    prog::{Call, Prog},
    target::Target,
    ty::{BinaryFormat, Dir, TypeKind},
    value::{PtrValue, ResValueId, ResValueKind, Value, ValueKind, VmaValue},
};
use iota::iota;
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
const EXEC_MAX_COMMANDS: u64 = 1000;

#[derive(Debug, Error)]
pub enum SerializeError {
    #[error("buffer two small to serialize the prog, provided size: {provided} bytes")]
    BufferTooSmall { provided: usize },
}

/// Serialize a prog into packed binary format.
pub fn serialize(target: &Target, p: &Prog, buf: &mut [u8]) -> Result<usize, SerializeError> {
    let mut ctx = ExecCtx {
        target,
        buf,
        res_args: HashMap::default(),
        copyout_seq: 0,
        eof: false,
    };

    for call in p.calls() {
        ctx.serialize_call(call);
    }
    ctx.write_u64(EXEC_INSTR_EOF);

    if !ctx.buf.has_remaining_mut() || ctx.eof || ctx.copyout_seq > EXEC_MAX_COMMANDS {
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
    res_args: HashMap<ResValueId, ArgInfo>,
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
    pub fn with_idx(idx: u64) -> Self {
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
        self.write_u64(c.sid() as u64);
        if let Some(ret) = c.ret.as_ref() {
            let val = ret.checked_as_res();
            let id = val.res_val_id().unwrap();
            let new_id = self.copyout_seq;
            self.copyout_seq += 1;
            self.res_args.insert(id, ArgInfo::with_idx(new_id));
            self.write_u64(new_id);
        } else {
            self.write_u64(EXEC_NO_COPYOUT);
        }
        self.write_u64(c.args().len() as u64);
        for val in c.args() {
            self.write_arg(val);
        }
        self.write_copyout(c);
    }

    fn write_copyin(&mut self, c: &Call) {
        foreach_call_args(self.target, c, |val, ctx| {
            if ctx.base.is_none() {
                return;
            }
            let base = ctx.base.unwrap();
            let mut val_addr = base + ctx.offset;
            let ty = val.ty(ctx.target);
            val_addr -= ty.bitfield_unit_off();

            if let Some(res_val) = val.as_res() {
                if let ResValueKind::Own(id) = &res_val.kind {
                    self.res_args.insert(*id, ArgInfo::with_addr(val_addr));
                }
            }

            if let ValueKind::Group | ValueKind::Union = &val.kind() {
                return;
            }

            let ty = val.ty(ctx.target);
            if val.dir() == Dir::Out
                || (ty.as_const().is_some() && ty.checked_as_const().pad())
                || (val.size(ctx.target) == 0 && !ty.is_bitfield())
            {
                return;
            }

            self.write_u64(EXEC_INSTR_COPY_IN);
            self.write_u64(val_addr);
            self.write_arg(val);
        })
    }

    fn write_copyout(&mut self, c: &Call) {
        foreach_call_args(self.target, c, |val, _| {
            if val.as_res().is_none() {
                return;
            }
            let val = val.checked_as_res();
            if val.own_res() {
                let id = val.res_val_id().unwrap();
                let info = self.res_args.get_mut(&id).unwrap();
                if info.ret {
                    return;
                }
                let new_idx = self.copyout_seq;
                self.copyout_seq += 1;
                info.idx = new_idx;
                let addr = info.addr;

                self.write_u64(EXEC_INSTR_COPY_OUT);
                self.write_u64(new_idx);
                self.write_u64(addr);
                self.write_u64(val.size(self.target));
            }
        })
    }

    fn write_arg(&mut self, val: &Value) {
        match val.kind() {
            ValueKind::Integer => {
                let val = val.checked_as_int();
                let (int_val, pid_stride) = val.value(self.target);
                let ty = val.ty(self.target);
                self.write_const_arg(
                    ty.bitfield_unit(),
                    int_val,
                    ty.bitfield_off(),
                    ty.bitfield_len(),
                    pid_stride,
                    ty.format(),
                );
            }
            ValueKind::Res => {
                let val = val.checked_as_res();
                match &val.kind {
                    ResValueKind::Own(_) | ResValueKind::Null => {
                        let ty = val.ty(self.target);
                        self.write_const_arg(val.size(self.target), val.val, 0, 0, 0, ty.format());
                    }
                    ResValueKind::Ref(idx) => {
                        self.write_u64(EXEC_ARG_RESULT);
                        let ty = val.ty(self.target);
                        let meta = val.size(self.target) | (ty.format() as u64) << 8;
                        self.write_u64(meta);
                        self.write_u64(*idx);
                        self.write_u64(val.op_div);
                        self.write_u64(val.op_add);
                        self.write_u64(val.val);
                    }
                }
            }
            ValueKind::Ptr => {
                let val = val.checked_as_ptr();
                let ty = val.ty(self.target);
                self.write_const_arg(
                    val.size(self.target),
                    ptr_physical_addr(self.target, val),
                    0,
                    0,
                    0,
                    ty.format(),
                );
            }
            ValueKind::Vma => {
                let val = val.checked_as_vma();
                let ty = val.ty(self.target);
                self.write_const_arg(
                    val.size(self.target),
                    vma_physical_addr(self.target, val),
                    0,
                    0,
                    0,
                    ty.format(),
                );
            }
            ValueKind::Data => {
                let val = val.checked_as_data();
                let ty = val.ty(self.target);
                let bs = &val.data;

                if bs.is_empty() {
                    return;
                }
                self.write_u64(EXEC_ARG_DATA);
                let mut flags = bs.len() as u64;
                if matches!(ty.kind(), TypeKind::BufferFilename | TypeKind::BufferString) {
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
            ValueKind::Union => {
                let val = val.checked_as_union();
                self.write_arg(&val.option)
            }
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
        bf: BinaryFormat,
    ) {
        self.write_u64(EXEC_ARG_CONST);
        let meta = sz | (bf as u64) << 8 | bf_offset << 16 | bf_len << 24 | pid_stride << 32;
        self.write_u64(meta);
        self.write_u64(val);
    }

    fn write_u64(&mut self, val: u64) {
        if self.buf.len() >= 8 {
            if cfg!(target_endian = "little") {
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
}
#[derive(Clone)]
struct ArgCtx<'a> {
    target: &'a Target,
    base: Option<u64>,
    offset: u64,
}

fn foreach_call_args(target: &Target, call: &Call, mut f: impl FnMut(&Value, &ArgCtx)) {
    let mut ctx = ArgCtx {
        target,
        base: None,
        offset: 0,
    };
    if let Some(ret) = call.ret.as_ref() {
        foreach_arg(ret, &mut ctx, &mut f)
    }
    for val in call.args() {
        foreach_arg(val, &mut ctx, &mut f)
    }
}

fn foreach_arg(val: &Value, ctx: &mut ArgCtx, f: &mut dyn FnMut(&Value, &ArgCtx)) {
    let ctx_backup = ctx.clone();

    f(val, ctx);

    match &val.kind() {
        ValueKind::Group => {
            let val = val.checked_as_group();
            for inner in &val.inner {
                foreach_arg(inner, ctx, f);
                ctx.offset += inner.size(ctx.target);
            }
        }
        ValueKind::Union => {
            let val = val.checked_as_union();
            foreach_arg(&val.option, ctx, f);
        }
        ValueKind::Ptr => {
            let val = val.checked_as_ptr();
            if let Some(pointee) = val.pointee.as_ref() {
                ctx.base = Some(ptr_physical_addr(ctx.target, val));
                ctx.offset = 0;
                foreach_arg(pointee, ctx, f);
            }
        }
        _ => {}
    }

    *ctx = ctx_backup;
}

#[inline]
fn ptr_physical_addr(target: &Target, ptr: &PtrValue) -> u64 {
    if ptr.is_special() {
        let idx = (-(ptr.addr as i64)) as usize;
        target.special_ptrs()[idx]
    } else {
        target.data_offset() + ptr.addr
    }
}

#[inline]
fn vma_physical_addr(target: &Target, vma: &VmaValue) -> u64 {
    if vma.is_special() {
        let idx = (-(vma.addr as i64)) as usize;
        target.special_ptrs()[idx]
    } else {
        target.data_offset() + vma.addr
    }
}
