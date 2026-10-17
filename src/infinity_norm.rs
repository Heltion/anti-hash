//! Exact one-dimensional reductions for the infinity norm. This is a local
//! special case inspired by generalized basis reduction, not full GBR: there
//! are no quotient-norm LPs or global shortest-vector guarantees here.
//!
//! Inspired by Lovász–Scarf, "The Generalized Basis Reduction Algorithm"
//! (1992), https://doi.org/10.1287/moor.17.3.751 . The implemented objective is
//! max |v_j| on exact zero-residue vectors, using only integer row additions.
use malachite::base::num::arithmetic::traits::{Abs, DivRound};
use malachite::base::rounding_modes::RoundingMode::{Ceiling, Floor};
use malachite::Integer;

/// Find an integer t with ||v + t*w||_infinity <= radius. Every coordinate
/// gives an exact closed integer interval; their intersection is feasible iff
/// such a t exists. No floating-point arithmetic or coefficient cutoff is used.
pub(crate) fn bounded_step(v: &[Integer], w: &[Integer], radius: &Integer) -> Option<Integer> {
    assert_eq!(v.len(), w.len());
    let mut lower: Option<Integer> = None;
    let mut upper: Option<Integer> = None;
    for (v, w) in v.iter().zip(w) {
        if w == &Integer::from(0) {
            if v.abs() > *radius {
                return None;
            }
            continue;
        }
        let left = -radius.clone() - v;
        let right = radius - v;
        let (lo, hi) = if w > &Integer::from(0) {
            (left.div_round(w, Ceiling).0, right.div_round(w, Floor).0)
        } else {
            (right.div_round(w, Ceiling).0, left.div_round(w, Floor).0)
        };
        if lower.as_ref().is_none_or(|x| lo > *x) {
            lower = Some(lo);
        }
        if upper.as_ref().is_none_or(|x| hi < *x) {
            upper = Some(hi);
        }
        if lower > upper {
            return None;
        }
    }
    // The closest feasible coefficient to zero avoids unnecessary row growth.
    match (lower, upper) {
        (Some(lo), Some(_)) if lo > 0 => Some(lo),
        (Some(_), Some(hi)) if hi < 0 => Some(hi),
        _ => Some(Integer::from(0)),
    }
}

/// Minimize ||v + t*w||_infinity over integer t, provided strict improvement
/// is possible. Integer radii make feasibility monotone, so binary search finds
/// the global minimum on this line even when the optimal coefficient is huge.
pub(crate) fn improving_step(v: &[Integer], w: &[Integer]) -> Option<Integer> {
    let maximum = v.iter().map(Abs::abs).max()?;
    if maximum == 0 {
        return None;
    }
    let mut high = maximum - Integer::from(1);
    let mut best = bounded_step(v, w, &high)?;
    let mut low = Integer::from(0);
    while low < high {
        let mid = (&low + &high).div_round(Integer::from(2), Floor).0;
        if let Some(step) = bounded_step(v, w, &mid) {
            high = mid;
            best = step;
        } else {
            low = mid + Integer::from(1);
        }
    }
    Some(best)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn norm(v: &[Integer]) -> Integer {
        v.iter().map(Abs::abs).max().unwrap()
    }
    fn combine(v: &[Integer], w: &[Integer], t: &Integer) -> Vec<Integer> {
        v.iter().zip(w).map(|(v, w)| v + t * w).collect()
    }

    #[test]
    fn interval_and_minimum_match_exhaustive_integer_search() {
        let mut state = 987654321u64;
        for _ in 0..500 {
            let mut next = || {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state
            };
            let v: Vec<Integer> = (0..3)
                .map(|_| Integer::from((next() % 11) as i32 - 5))
                .collect();
            let w: Vec<Integer> = (0..3)
                .map(|_| Integer::from((next() % 7) as i32 - 3))
                .collect();
            // For ||v||∞<=5, any improving coefficient has |t|<=10
            // whenever w is nonzero. This independent range is exhaustive.
            let exact = (-20..=20)
                .map(|t| norm(&combine(&v, &w, &Integer::from(t))))
                .min()
                .unwrap();
            let step = improving_step(&v, &w);
            assert_eq!(step.is_some(), exact < norm(&v));
            if let Some(t) = step {
                assert_eq!(norm(&combine(&v, &w, &t)), exact);
            }
            for r in 0..=6 {
                let radius = Integer::from(r);
                let t = bounded_step(&v, &w, &radius);
                assert_eq!(t.is_some(), exact <= radius);
                if let Some(t) = t {
                    assert!(norm(&combine(&v, &w, &t)) <= radius);
                }
            }
        }
    }

    #[test]
    fn supports_coefficients_beyond_f64_range() {
        let k: Integer = format!("1{}", "0".repeat(400)).parse().unwrap();
        let v = vec![k.clone(), &k + Integer::from(1)];
        let w = vec![Integer::from(1); 2];
        let t = improving_step(&v, &w).unwrap();
        assert_eq!(norm(&combine(&v, &w, &t)), 1);
        assert!(t.abs() >= k);
    }

    #[test]
    fn infinity_improvement_can_increase_euclidean_norm() {
        let v = vec![6, 0, -2]
            .into_iter()
            .map(Integer::from)
            .collect::<Vec<_>>();
        let w = vec![-1, 5, 7]
            .into_iter()
            .map(Integer::from)
            .collect::<Vec<_>>();
        let t = improving_step(&v, &w).unwrap();
        let improved = combine(&v, &w, &t);
        assert_eq!(norm(&improved), 5);
        let square = |v: &[Integer]| v.iter().map(|x| x * x).sum::<Integer>();
        assert!(square(&improved) > square(&v));
    }
}
