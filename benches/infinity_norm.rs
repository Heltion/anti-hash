#![feature(test)]

extern crate test;

use anti_hash::{
    anti_hash_with_bkz, anti_hash_with_infinity_norm, AntiResult, BkzOptions, Parameters,
};
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

fn run(p: &Parameters, block_size: usize, infinity: bool) -> AntiResult {
    let options = BkzOptions {
        block_size,
        max_tours: 4,
    };
    if infinity {
        anti_hash_with_infinity_norm(p.clone(), options)
    } else {
        anti_hash_with_bkz(p.clone(), options)
    }
}

fn succeeds(result: AntiResult, p: &Parameters) -> bool {
    match result {
        AntiResult::Ok(_, a, b) => {
            validate(&a, &b, p);
            true
        }
        AntiResult::NotFound(..) => false,
        AntiResult::TimeOut(..) => panic!("timed out"),
        AntiResult::Unknown => panic!("invalid collision"),
    }
}

#[test]
fn boundary_corpus() {
    let mut successes = [0usize; 4];
    let mut times = [0f64; 4];
    let modes = [(2, false), (2, true), (20, false), (20, true)];
    for palindrome in [false, true] {
        for pairs in 1..=3 {
            let first = [6, 11, 16][pairs - 1];
            for n in first..first + 4 {
                for seed in 0..64 {
                    let length = if palindrome { 2 * n } else { n };
                    let p = parameters(pairs, length, seed, palindrome);
                    let mut found = [false; 4];
                    for (i, (block, infinity)) in modes.iter().copied().enumerate() {
                        let start = std::time::Instant::now();
                        let result = run(&p, block, infinity);
                        times[i] += start.elapsed().as_secs_f64();
                        found[i] = succeeds(result, &p);
                        successes[i] += usize::from(found[i]);
                    }
                    assert!(!found[0] || found[1]);
                    assert!(!found[2] || found[3]);
                    if !found[2] && found[3] {
                        println!("BKZ+infinity win: reverse={palindrome} pairs={pairs} length={length} seed={seed}");
                    }
                    if !found[0] && found[1] {
                        println!("L2+infinity win: reverse={palindrome} pairs={pairs} length={length} seed={seed}");
                    }
                }
            }
        }
    }
    println!("successes: {successes:?}; total seconds: {times:?}");
    assert_eq!(successes, [1473, 1481, 1477, 1484]);
}

fn measure(b: &mut Bencher, p: Parameters, block: usize, infinity: bool, expected: bool) {
    assert_eq!(succeeds(run(&p, block, infinity), &p), expected);
    b.iter(|| black_box(run(black_box(&p), black_box(block), black_box(infinity))));
}

macro_rules! case {
    ($name:ident, $pairs:expr, $length:expr, $seed:expr, $reverse:expr, $block:expr, $infinity:expr, $expected:expr) => {
        #[bench]
        fn $name(b: &mut Bencher) {
            measure(
                b,
                parameters($pairs, $length, $seed, $reverse),
                $block,
                $infinity,
                $expected,
            );
        }
    };
}

// Each iteration is one complete solve for one fixed set of hash parameters.
// A fast NotFound is not equivalent to solving the input: assertions distinguish
// effectiveness, while the Bencher measures the complete call's latency.
case!(two_hashes_l2, 2, 11, 44, false, 2, false, false);
case!(two_hashes_l2_infinity, 2, 11, 44, false, 2, true, true);
case!(two_hashes_bkz, 2, 11, 44, false, 20, false, false);
case!(two_hashes_bkz_infinity, 2, 11, 44, false, 20, true, true);
case!(three_hashes_l2, 3, 16, 6, false, 2, false, false);
case!(three_hashes_l2_infinity, 3, 16, 6, false, 2, true, true);
case!(three_hashes_bkz, 3, 16, 6, false, 20, false, false);
case!(three_hashes_bkz_infinity, 3, 16, 6, false, 20, true, true);
case!(reverse_bkz, 3, 32, 24, true, 20, false, false);
case!(reverse_bkz_infinity, 3, 32, 24, true, 20, true, true);
case!(easy_l2, 1, 20, 0, false, 2, false, true);
case!(easy_l2_infinity, 1, 20, 0, false, 2, true, true);
