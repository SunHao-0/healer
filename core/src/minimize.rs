use crate::prog::Prog;

pub fn minimize<F>(_p: &Prog, _eq: F) -> Prog
where
    F: Fn(&Prog) -> bool,
{
    todo!()
}
