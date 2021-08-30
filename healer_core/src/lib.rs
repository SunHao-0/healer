//! Core algorithms and data structures of healer

#[macro_use]
extern crate pest_derive;

use ahash::{AHashMap, AHashSet};

#[macro_use]
pub mod verbose;
pub mod alloc;
pub mod context;
pub mod corpus;
pub mod gen;
pub mod len;
pub mod mutation;
pub mod parse;
pub mod prog;
pub mod relation;
pub mod select;
pub mod syscall;
pub mod target;
pub mod ty;
pub mod value;
pub mod value_pool;

pub type HashMap<K, V> = AHashMap<K, V>;
pub type HashSet<V> = AHashSet<V>;
pub type RngType = rand::rngs::SmallRng;

pub fn foo() {
    println!("hello healer");
}
