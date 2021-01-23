use super::*;
use crate::targets::Target;

/// Select a syscall to fuzz based on resource usage.
pub(super) fn select_syscall(ctx: &GenContext) -> SyscallRef {
    loop {
        let call = if should_try_gen_res(ctx) {
            let res_ty = select_res(&ctx.target.res_tys);
            if let Some(call) = select_res_producer(ctx.target, res_ty) {
                call
            } else {
                continue;
            }
        } else {
            select_syscall_rand(ctx)
        };
        if !call.attr.disable {
            return call;
        }
    }
}

fn should_try_gen_res(ctx: &GenContext) -> bool {
    // Since the length of a test case is [4, 16), the number
    // of generated resource should be [2, 6)
    const MIN_RES_NUMBER: usize = 2;
    const MAX_RES_NUMBER: usize = 6;
    let res_count = ctx.generated_res.len();
    if res_count == 0 {
        // always make sure we start from generating a resource.
        true
    } else if res_count >= MAX_RES_NUMBER {
        random::<f32>() < 0.2 * (MAX_RES_NUMBER as f32 / (res_count as f32 * 2.0))
    } else {
        let alpha = 1.0 - (res_count as f32) / (MAX_RES_NUMBER as f32);
        if res_count < MIN_RES_NUMBER {
            random::<f32>() < 0.8 * alpha
        } else {
            random::<f32>() < 0.4 * alpha
        }
    }
}

fn select_syscall_rand(ctx: &GenContext) -> SyscallRef {
    // Try to select a consumer first.
    if random::<f32>() < 0.96 {
        if let Some(syscall) = ctx
            .generated_res
            .iter()
            .flat_map(|(res_ty, _)| res_ty.res_desc().unwrap().consumers.iter())
            .choose(&mut thread_rng())
        {
            return syscall;
        }
    }
    ctx.target
        .syscalls
        .iter()
        .choose(&mut thread_rng())
        .unwrap()
}

fn select_res(res_tys: &[TypeRef]) -> TypeRef {
    *res_tys.iter().choose(&mut thread_rng()).unwrap()
}

fn select_res_producer(t: &Target, res: TypeRef) -> Option<SyscallRef> {
    let res_desc = res.res_desc().unwrap();
    let subtypes = &t.subtype_map[&res];
    let supertypes = &t.supertype_map[&res];
    let accurate_ctors = &res_desc.ctors;
    let mut rng = thread_rng();

    if !accurate_ctors.is_empty() && rng.gen::<f32>() < 0.85 {
        // Try to choose calls that generate current resource and do not depend on other resources first.
        if let Some(e) = accurate_ctors
            .iter()
            .filter(|s| s.input_res.is_empty())
            .choose(&mut rng)
        {
            if rng.gen::<f32>() < 0.8 {
                return Some(e);
            }
        }
        accurate_ctors.iter().copied().choose(&mut rng)
    } else if !subtypes.is_empty() && rng.gen::<f32>() < 0.75 {
        let subtype = subtypes.choose(&mut rng)?;
        subtype
            .res_desc()
            .unwrap()
            .ctors
            .iter()
            .copied()
            .choose(&mut rng)
    } else {
        let supertype = supertypes.choose(&mut rng)?;
        supertype
            .res_desc()
            .unwrap()
            .ctors
            .iter()
            .copied()
            .choose(&mut rng)
    }
}
