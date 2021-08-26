//! Relation learning algorithm.

use crate::{prog::Prog, syscall::SyscallId, target::Target, HashMap};

/// Influence relations between syscalls.
#[derive(Debug, Clone)]
pub struct Relation {
    influence: HashMap<SyscallId, Vec<SyscallId>>,
    influence_by: HashMap<SyscallId, Vec<SyscallId>>,
    n: usize,
}

impl Relation {
    /// Create initial relations based on syscall type information.
    pub fn new(target: &Target) -> Self {
        let influence: HashMap<SyscallId, Vec<SyscallId>> = target
            .enabled_syscalls()
            .iter()
            .map(|syscall| (syscall.id(), Vec::new()))
            .collect();
        let influence_by = influence.clone();
        let mut r = Relation {
            influence,
            influence_by,
            n: 0,
        };

        for i in target.enabled_syscalls().iter().map(|s| s.id()) {
            for j in target.enabled_syscalls().iter().map(|s| s.id()) {
                if i != j && Self::calculate_influence(target, i, j) {
                    r.push_ordered(i, j);
                }
            }
        }

        r
    }

    /// Calculate if syscall `a` can influence the execution of syscall `b` based on
    /// input/output resources.
    ///
    /// Syscall `a` can influcen syscall `b` when any resource output by `a` is subtype
    /// of resources input by `b`. For example, syscall `a` outputs resource `sock`, syscall
    /// `b` takes resource `fd` as input, then `a` can influence `b`, because `sock` is
    /// subtype of `fd`. In contrast, if `b` takes `sock_ax25` as input, then the above
    /// conlusion maybe wrong (return false), because `sock` is not subtype of `sock_ax25` and
    /// the output resource of `a` maybe useless for `b`. For the latter case, the relation
    /// should be judged with dynamic method.
    pub fn calculate_influence(target: &Target, a: SyscallId, b: SyscallId) -> bool {
        let output_res_a = target.syscall_output_res(a);
        let input_res_b = target.syscall_input_res(b);

        !output_res_a.is_empty()
            && !input_res_b.is_empty()
            && input_res_b.iter().any(|input_res| {
                output_res_a
                    .iter()
                    .any(|output_res| target.res_sub_tys(input_res).contains(output_res))
            })
    }

    /// Detect relations by removing calls dynamically.
    ///
    /// The algorithm removes each call of `p` except for the last one and calls the callback
    /// `changed` to verify if the removal changed the feedback of adjacent call.
    /// For example, for prog [open, read], the algorithm removes `open` first and calls `changed`
    /// with the index of `open` (0 in this case) and the `new_prog`. The index of `open` equals to
    /// the index of `read` in the new `prog` and the callback `changed` should judge the feedback
    /// changes of the `index` call after the execution of `new_prog`. Finally, `try_update` returns
    /// the number of detected new relations.
    pub fn try_update<T>(&mut self, p: &Prog, mut pred: T) -> usize
    where
        T: FnMut(&Prog, usize) -> bool, // fn(new_prog: &Prog, index: usize) -> bool
    {
        let mut n = 0;
        for (i, adjacent_calls) in p.calls().windows(2).enumerate() {
            let a = &adjacent_calls[0];
            let b = &adjacent_calls[1];
            if !self.influence(a.sid(), b.sid()) {
                let new_p = p.remove_call(i);
                if pred(&new_p, i) {
                    self.push_ordered(a.sid(), b.sid());
                    n += 1;
                }
            }
        }
        n
    }

    /// Return if `a` can influence the execution of `b`.
    #[inline]
    pub fn influence(&self, a: SyscallId, b: SyscallId) -> bool {
        self.influence[&a].binary_search(&b).is_ok()
    }

    /// Return if `a` can be influenced by the execution of `b`.
    #[inline]
    pub fn influence_by(&self, a: SyscallId, b: SyscallId) -> bool {
        self.influence_by[&a].binary_search(&b).is_ok()
    }

    /// Return the known syscalls that `a` can influence.
    #[inline]
    pub fn influence_of(&self, a: SyscallId) -> &[SyscallId] {
        &self.influence[&a]
    }

    /// Return the known syscalls that can influence `a`.
    #[inline]
    pub fn influence_by_of(&self, a: SyscallId) -> &[SyscallId] {
        &self.influence_by[&a]
    }

    #[inline(always)]
    pub fn influences(&self) -> &HashMap<SyscallId, Vec<SyscallId>> {
        &self.influence
    }

    #[inline(always)]
    pub fn influences_by(&self) -> &HashMap<SyscallId, Vec<SyscallId>> {
        &self.influence_by
    }

    /// Return the number of known relations.
    #[inline(always)]
    pub fn num(&self) -> usize {
        self.n
    }

    fn push_ordered(&mut self, a: SyscallId, b: SyscallId) {
        let rs_a = self.influence.get_mut(&a).unwrap();
        if let Err(idx) = rs_a.binary_search(&b) {
            self.n += 1;
            rs_a.insert(idx, b);
        }
        let rs_b = self.influence_by.get_mut(&b).unwrap();
        if let Err(idx) = rs_b.binary_search(&a) {
            rs_b.insert(idx, a);
        }
    }
}
