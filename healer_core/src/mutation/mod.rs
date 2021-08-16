//! Prog mutation

use crate::{
    context::Context, corpus::CorpusWrapper, prog::Prog, relation::Relation, target::Target,
    value::Value, RngType,
};

pub mod buffer;
pub mod group;
pub mod int;
pub mod ptr;
pub mod res;

pub fn mutate(
    _target: &Target,
    _relation: &Relation,
    _corpus: &CorpusWrapper,
    _rng: &mut RngType,
    _p: &mut Prog,
) {
    todo!()
}

pub fn mutate_value(_ctx: &mut Context, _rng: &mut RngType, _val: &mut Value) {}
