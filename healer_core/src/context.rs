use std::fmt::Display;

use crate::{
    alloc::{Allocator, VmaAllocator},
    prog::{Call, Prog},
    relation::RelationWrapper,
    target::Target,
    ty::ResKind,
    value::ResValueId,
    HashMap,
};

/// A context records useful information of multiple calls.
#[derive(Debug, Clone)]
pub struct Context<'a, 'b> {
    /// Fuzzing target of current prog.
    pub(crate) target: &'a Target,
    /// Relations between syscalls.
    pub(crate) relation: &'b RelationWrapper,
    /// Dummy mem allocator.
    pub(crate) mem_allocator: Allocator,
    /// Dummy vma allocator.
    pub(crate) vma_allocator: VmaAllocator,
    /// Next avaliable resource id.
    pub(crate) next_res_id: u64,
    /// Generated res kind.
    pub(crate) res_kinds: Vec<ResKind>,
    /// Generated res kind&id mapping.
    pub(crate) res_ids: HashMap<ResKind, Vec<ResValueId>>,
    /// Generated strings.
    pub(crate) strs: Vec<Vec<u8>>,
    /// Generated filenames.
    pub(crate) filenames: Vec<Vec<u8>>,
    /// Calls of current context.
    pub(crate) calls: Vec<Call>,
}

impl<'a, 'b> Context<'a, 'b> {
    /// Create an empty context with `target` and `relation`.
    pub fn new(target: &'a Target, relation: &'b RelationWrapper) -> Self {
        Self {
            target,
            relation,
            mem_allocator: Allocator::new(target.mem_size()),
            vma_allocator: VmaAllocator::new(target.page_num()),
            next_res_id: 0,
            res_kinds: Vec::new(),
            res_ids: HashMap::new(),
            strs: Vec::new(),
            filenames: Vec::new(),
            calls: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn dummy() -> Self {
        let dummy = &0;
        let target: &'static Target = unsafe { std::mem::transmute(dummy) };
        let relation: &'static RelationWrapper = unsafe { std::mem::transmute(dummy) };
        Self {
            target,
            relation,
            mem_allocator: Allocator::new(1024),
            vma_allocator: VmaAllocator::new(1024),
            next_res_id: 0,
            res_kinds: Vec::new(),
            res_ids: HashMap::new(),
            strs: Vec::new(),
            filenames: Vec::new(),
            calls: Vec::new(),
        }
    }

    /// Get current target of context.
    #[inline(always)]
    pub fn target(&self) -> &'a Target {
        self.target
    }

    /// Get current relation of context.
    #[inline(always)]
    pub fn relation(&self) -> &RelationWrapper {
        self.relation
    }

    /// Get current calls of context.
    #[inline(always)]
    pub fn calls(&self) -> &[Call] {
        &self.calls[..]
    }

    /// Get generated resource kinds of context.
    #[inline(always)]
    pub fn res(&self) -> &[ResKind] {
        &self.res_kinds
    }

    /// Get generated resource kind&name mapping of context.
    #[inline(always)]
    pub fn res_ids(&self) -> &HashMap<ResKind, Vec<ResValueId>> {
        &self.res_ids
    }

    #[inline(always)]
    pub fn strs(&self) -> &[Vec<u8>] {
        &self.strs
    }

    #[inline(always)]
    pub fn filenames(&self) -> &[Vec<u8>] {
        &self.filenames
    }

    /// Get mutable ref to current mem allocator.
    #[inline(always)]
    pub fn mem_allocator(&mut self) -> &mut Allocator {
        &mut self.mem_allocator
    }

    /// Get mutable ref to current vma allocator.
    #[inline(always)]
    pub fn vma_allocator(&mut self) -> &mut VmaAllocator {
        &mut self.vma_allocator
    }

    /// Append a call to context
    #[inline]
    pub fn append_call(&mut self, call: Call) {
        self.calls.push(call)
    }

    /// Next avaliable resource id.
    #[inline]
    pub fn next_res_id(&mut self) -> u64 {
        let id = self.next_res_id;
        self.next_res_id += 1;
        id
    }

    /// Record a generate resource to context.
    pub fn record_res(&mut self, kind: &ResKind, id: ResValueId) {
        if !self.res_ids.contains_key(kind) {
            self.res_ids.insert(kind.clone(), Vec::new());
            self.res_kinds.push(kind.clone());
        }
        self.res_ids.get_mut(kind).unwrap().push(id);
    }

    pub fn record_str(&mut self, val: Vec<u8>) {
        self.strs.push(val);
    }

    #[allow(clippy::ptr_arg)] // binary search requires &Vec<u8>
    pub fn record_filename(&mut self, val: &Vec<u8>) -> bool {
        if let Err(idx) = self.filenames.binary_search(val) {
            self.filenames.insert(idx, val.clone());
            true
        } else {
            false
        }
    }

    /// Dump to prog.
    pub fn to_prog(self) -> Prog {
        Prog::new(self.calls)
    }
}

impl<'a, 'b> Display for Context<'a, 'b> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "target: {}", self.target.target_name())?;
        writeln!(f, "relation num: {}", self.relation.num())?;
        writeln!(f, "mem: {:?}", self.mem_allocator)?;
        writeln!(f, "vma: {:?}", self.vma_allocator)?;
        writeln!(f, "res num: {}", self.next_res_id)?;
        writeln!(f, "res:")?;
        for (kind, ids) in &self.res_ids {
            writeln!(f, "\t{}: {:?}", kind, ids)?;
        }
        writeln!(f, "str:")?;
        for val in &self.strs {
            writeln!(f, "\t{:?}", val)?;
        }
        writeln!(f, "filenames:")?;
        for fname in &self.filenames {
            writeln!(f, "\t{:?}", fname)?;
        }
        writeln!(f, "calls:")?;
        for call in &self.calls {
            writeln!(f, "{}", call.display(self.target))?
        }
        Ok(())
    }
}
