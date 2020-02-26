use crate::prog::Prog;

// TODO
pub fn minimize<F>(p: &Prog, eq: F) -> Prog
where
    F: FnOnce(&Prog) -> bool,
{
    eq(p);
    p.clone()
}
