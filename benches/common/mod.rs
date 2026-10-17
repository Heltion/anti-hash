use anti_hash::Parameters;
use malachite::{Float, Integer};

pub fn parameters(pairs: usize, length: usize, seed: u64, palindrome: bool) -> Parameters {
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

pub fn validate(a: &str, b: &str, p: &Parameters) {
    assert_ne!(a, b);
    assert_eq!(a.len(), p.length);
    assert_eq!(b.len(), p.length);
    assert!(
        a.bytes()
            .chain(b.bytes())
            .all(|c| c >= b'a' && (c - b'a') < p.sigma as u8)
    );
    if p.palindrome {
        assert_eq!(a.chars().rev().collect::<String>(), b);
    }
    // Verify output bytes directly with exact modular arithmetic.
    for (m, q) in p.modulo.iter().zip(&p.base) {
        let hash = |s: &str| {
            s.bytes()
                .rev()
                .fold(Integer::from(0), |h, c| (h * q + Integer::from(c)) % m)
        };
        assert_eq!(hash(a), hash(b));
    }
}
