//! Exact lattice construction and verified polynomial-hash collision search.
//! Native callers can use `search` without linking browser dependencies.

#[cfg(any(target_arch = "wasm32", test))]
mod auto_length;
mod clock;
mod infinity_norm;
mod reduction;
mod search;
#[cfg(target_arch = "wasm32")]
mod web;

pub use search::{
    Alphabet, AntiResult, BkzOptions, InputError, Parameters, SearchOptions, anti_hash,
    anti_hash_with_bkz, anti_hash_with_infinity_norm, anti_palindrome_hash, search,
};
