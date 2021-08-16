//! Core algorithms and data structures of healer

use ahash::{AHashMap, AHashSet};
use std::cell::Cell;

pub mod alloc;
pub mod context;
pub mod corpus;
pub mod gen;
pub mod len;
pub mod mutation;
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

thread_local! {
    static VERBOSE: Cell<bool> = Cell::new(false);
}

pub fn set_verbose(verbose: bool) {
    VERBOSE.with(|v| v.set(verbose))
}

#[inline(always)]
fn verbose() -> bool {
    VERBOSE.with(|v| v.get())
}

pub fn foo() {
    println!("hello healer");
}
