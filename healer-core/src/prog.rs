use crate::{
    syscall::SyscallId,
    target::Target,
    ty::ResKind,
    value::{ResValue, ResValueId, Value, ValueKind},
    verbose, HashMap, HashSet,
};

#[derive(Debug, Clone)]
pub struct Call {
    sid: SyscallId,
    args: Box<[Value]>,
    pub ret: Option<Value>,
    pub generated_res: HashMap<ResKind, Vec<ResValueId>>,
    pub used_res: HashMap<ResKind, Vec<ResValueId>>,
}

impl Call {
    #[inline(always)]
    pub fn sid(&self) -> SyscallId {
        self.sid
    }

    #[inline(always)]
    pub fn args(&self) -> &[Value] {
        &self.args
    }

    #[inline(always)]
    pub fn args_mut(&mut self) -> &mut [Value] {
        &mut self.args
    }

    #[inline(always)]
    pub fn display<'a, 'b>(&'a self, target: &'b Target) -> CallDisplay<'a, 'b> {
        CallDisplay { call: self, target }
    }
}

pub struct CallDisplay<'a, 'b> {
    call: &'a Call,
    target: &'b Target,
}

impl<'a, 'b> std::fmt::Display for CallDisplay<'a, 'b> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if verbose() {
            writeln!(
                f,
                "[generated res: {:?}, used res: {:?}]",
                self.call.generated_res, self.call.used_res
            )?;
        }
        let syscall = self.target.syscall_of(self.call.sid);
        if let Some(ret) = self.call.ret.as_ref() {
            let ret = ret.checked_as_res();
            write!(f, "r{} = ", ret.res_val_id().unwrap())?;
        }
        write!(f, "{}(", syscall.name())?;
        for (i, arg) in self.call.args.iter().enumerate() {
            write!(f, "{}", arg.display(self.target))?;
            if i != self.call.args.len() - 1 {
                write!(f, ", ")?;
            }
        }
        write!(f, ")")
    }
}

#[derive(Debug, Clone)]
pub struct CallBuilder {
    sid: SyscallId,
    args: Vec<Value>,
    ret: Option<Value>,
    generated_res: HashMap<ResKind, HashSet<ResValueId>>,
    used_res: HashMap<ResKind, HashSet<ResValueId>>,
}

impl CallBuilder {
    pub fn new(sid: SyscallId) -> Self {
        Self {
            sid,
            args: Vec::new(),
            ret: None,
            generated_res: HashMap::default(),
            used_res: HashMap::default(),
        }
    }

    pub fn arg(&mut self, arg: Value) -> &mut Self {
        self.args.push(arg);
        self
    }

    pub fn args<T: IntoIterator<Item = Value>>(&mut self, args: T) -> &mut Self {
        self.args.extend(args);
        self
    }

    pub fn ret(&mut self, ret: Option<Value>) -> &mut Self {
        self.ret = ret;
        self
    }

    pub fn record_res(&mut self, res: &ResKind, id: ResValueId) -> &mut Self {
        if !self.generated_res.contains_key(res) {
            self.generated_res.insert(res.clone(), HashSet::new());
        }
        self.generated_res.get_mut(res).unwrap().insert(id);
        self
    }

    pub fn used_res(&mut self, res: &ResKind, id: ResValueId) -> &mut Self {
        if !self.used_res.contains_key(res) {
            self.used_res.insert(res.clone(), HashSet::new());
        }
        self.used_res.get_mut(res).unwrap().insert(id);
        self
    }

    pub fn build(mut self) -> Call {
        self.args.shrink_to_fit();
        let mut generated_res: HashMap<ResKind, Vec<ResValueId>> = HashMap::default();
        for (kind, ids) in self.generated_res {
            generated_res.insert(kind, ids.into_iter().collect());
        }
        let mut used_res: HashMap<ResKind, Vec<ResValueId>> = HashMap::default();
        for (kind, ids) in self.used_res {
            used_res.insert(kind, ids.into_iter().collect());
        }
        generated_res.shrink_to_fit();
        used_res.shrink_to_fit();

        Call {
            sid: self.sid,
            args: self.args.into_boxed_slice(),
            ret: self.ret,
            generated_res,
            used_res,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Prog {
    calls: Vec<Call>,
}

impl Prog {
    #[inline(always)]
    pub fn new(calls: Vec<Call>) -> Self {
        Self { calls }
    }

    #[inline(always)]
    pub fn calls(&self) -> &[Call] {
        &self.calls
    }

    #[inline(always)]
    pub fn calls_mut(&mut self) -> &mut [Call] {
        &mut self.calls
    }

    #[inline(always)]
    pub fn display<'a, 'b>(&'a self, target: &'b Target) -> ProgDisplay<'a, 'b> {
        ProgDisplay { prog: self, target }
    }

    pub fn remove_call(&self, i: usize) -> Prog {
        let mut new_p = self.clone();
        new_p.remove_call_inplace(i);
        new_p
    }

    pub fn remove_call_inplace(&mut self, i: usize) {
        if i == self.calls.len() - 1 {
            self.calls.pop();
            return;
        }

        let removed_call = self.calls.remove(i);
        let removed_res = removed_call.generated_res;
        for c in &mut self.calls[i..] {
            let mut ids_to_remove = HashSet::new();
            for (removed_res_kind, removed_ids) in removed_res.iter() {
                if let Some(used_ids) = c.used_res.get(removed_res_kind) {
                    for id in used_ids {
                        if removed_ids.contains(id) {
                            ids_to_remove.insert(*id);
                        }
                    }
                }
            }
            if !ids_to_remove.is_empty() {
                c.args
                    .iter_mut()
                    .for_each(|arg| Self::set_res_val_to_null(arg, &ids_to_remove));
            }
        }
    }

    fn set_res_val_to_null(val: &mut Value, ids: &HashSet<ResValueId>) {
        match val.kind() {
            ValueKind::Res => {
                let val = val.checked_as_res_mut();
                if let Some(id) = val.res_val_id() {
                    if ids.contains(&id) {
                        assert!(val.is_ref());
                        *val = ResValue::new_null(val.ty_id(), val.dir(), 0);
                    }
                }
            }
            ValueKind::Ptr => {
                let val = val.checked_as_ptr_mut();
                if let Some(pointee) = val.pointee.as_mut() {
                    Self::set_res_val_to_null(pointee, ids);
                }
            }
            ValueKind::Group => {
                let vals = val.checked_as_group_mut();
                for val in vals.inner.iter_mut() {
                    Self::set_res_val_to_null(val, ids);
                }
            }
            ValueKind::Union => {
                let val = val.checked_as_union_mut();
                Self::set_res_val_to_null(&mut val.option, ids);
            }
            _ => (),
        }
    }
}

pub struct ProgDisplay<'a, 'b> {
    prog: &'a Prog,
    target: &'b Target,
}

impl<'a, 'b> std::fmt::Display for ProgDisplay<'a, 'b> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for call in &self.prog.calls {
            writeln!(f, "{}", call.display(self.target))?;
        }
        Ok(())
    }
}
