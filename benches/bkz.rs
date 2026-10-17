#![feature(test)]

extern crate test;

use anti_hash::{anti_hash_with_bkz, AntiResult, BkzOptions, Parameters};
use malachite::Integer;
use malachite_float::Float;
use std::hint::black_box;
use test::Bencher;

fn parameters(pairs: usize, length: usize, seed: u64, palindrome: bool) -> Parameters {
    Parameters {
        length,
        modulo: [998244353u64, 1000000007, 1000000009][..pairs]
            .iter()
            .copied()
            .map(Integer::from)
            .collect(),
        base: [233 + seed * 2, 911382323 - seed * 4, 972663749 - seed * 6][..pairs]
            .iter()
            .copied()
            .map(Integer::from)
            .collect(),
        lambda: Integer::from(100000),
        delta: Float::from(0.99),
        eta: Float::from(0.51),
        precision: 40,
        palindrome,
        sigma: 26,
        timeout: 2.,
    }
}

fn validate(a: &str, b: &str, p: &Parameters) {
    assert_ne!(a, b);
    assert_eq!(a.len(), p.length);
    assert_eq!(b.len(), p.length);
    assert!(a
        .bytes()
        .chain(b.bytes())
        .all(|c| c >= b'a' && (c - b'a') < p.sigma as u8));
    if p.palindrome {
        assert_eq!(a.chars().rev().collect::<String>(), b);
    }
    // Horner evaluation is independent of the production powers()/check().
    for (m, q) in p.modulo.iter().zip(&p.base) {
        let hash = |s: &str| {
            s.bytes()
                .rev()
                .fold(Integer::from(0), |h, c| (h * q + Integer::from(c)) % m)
        };
        assert_eq!(hash(a), hash(b));
    }
}

fn run(p: &Parameters, block_size: usize) -> AntiResult {
    anti_hash_with_bkz(
        p.clone(),
        BkzOptions {
            block_size,
            max_tours: 4,
        },
    )
}

fn succeeds(result: AntiResult, p: &Parameters) -> bool {
    match result {
        AntiResult::Ok(_, a, b) => {
            validate(&a, &b, p);
            true
        }
        AntiResult::NotFound(..) => false,
        AntiResult::TimeOut(..) => panic!("benchmark input timed out"),
        AntiResult::Unknown => panic!("invalid collision"),
    }
}

fn measure(b: &mut Bencher, p: Parameters, block_size: usize, expected: bool) {
    // Validate once outside timing. Production modular verification remains
    // inside the measured algorithm. Cloning is included in both methods.
    assert_eq!(succeeds(run(&p, block_size), &p), expected);
    b.iter(|| black_box(run(black_box(&p), black_box(block_size))));
}

macro_rules! compare {
    ($name:ident, $pairs:expr, $length:expr, $seed:expr, $reverse:expr, $l2_ok:expr) => {
        mod $name {
            use super::*;
            #[bench]
            fn l2(b: &mut Bencher) {
                measure(b, parameters($pairs, $length, $seed, $reverse), 2, $l2_ok);
            }
            #[bench]
            fn bkz20(b: &mut Bencher) {
                measure(b, parameters($pairs, $length, $seed, $reverse), 20, true);
            }
        }
    };
}

// Fixed effectiveness regressions, plus an easy input already solved by L².
compare!(normal_two_hashes, 2, 11, 39, false, false);
compare!(normal_three_hashes, 3, 16, 26, false, false);
compare!(reverse_seed31, 3, 32, 31, true, false);
compare!(reverse_seed46, 3, 32, 46, true, false);
compare!(easy, 1, 20, 0, false, true);

// Run as a test, not a timing benchmark: the complete deterministic corpus
// measures effectiveness; the #[bench] cases above measure per-input latency.
#[test]
fn boundary_corpus_success_counts() {
    let mut successes = [0usize; 2];
    for palindrome in [false, true] {
        for pairs in 1..=3 {
            let first_length = [6, 11, 16][pairs - 1];
            for n in first_length..first_length + 4 {
                let length = if palindrome { 2 * n } else { n };
                for seed in 0..64 {
                    let p = parameters(pairs, length, seed, palindrome);
                    let found = [succeeds(run(&p, 2), &p), succeeds(run(&p, 20), &p)];
                    assert!(!found[0] || found[1], "BKZ lost an L² success");
                    for i in 0..2 {
                        successes[i] += usize::from(found[i]);
                    }
                }
            }
        }
    }
    assert_eq!(successes, [1473, 1477]);
    println!(
        "boundary corpus (1536 inputs): L²={}, BKZ-20={}",
        successes[0], successes[1]
    );
}
