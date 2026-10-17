use super::bkz::Enumeration;
use super::*;
use crate::*;

fn reducer(b: Vec<Vec<i32>>) -> Reducer {
    Reducer::new(
        b.into_iter()
            .map(|r| r.into_iter().map(Integer::from).collect())
            .collect(),
        Float::from(0.99),
        Float::from(0.51),
        40,
        0,
        30.,
        0,
    )
}

fn determinant(b: &[Vec<Integer>]) -> Integer {
    if b.len() == 1 {
        return b[0][0].clone();
    }
    let mut result = Integer::from(0);
    for j in 0..b.len() {
        let minor: Vec<Vec<Integer>> = b[1..]
            .iter()
            .map(|r| {
                r.iter()
                    .enumerate()
                    .filter(|(k, _)| *k != j)
                    .map(|(_, x)| x.clone())
                    .collect()
            })
            .collect();
        let term = &b[0][j] * determinant(&minor);
        if j % 2 == 0 {
            result += term;
        } else {
            result -= term;
        }
    }
    result
}

#[test]
fn infinity_reduction_preserves_lattice_and_zero_residues() {
    let mut l = reducer(vec![
        vec![0, 6, 0, -2],
        vec![0, -1, 5, 7],
        vec![0, 0, 9, 1],
        vec![101, 0, 0, 0],
    ]);
    l.residue_columns = 1;
    l.sigma = 6;
    let det = determinant(&l.b).abs();
    assert!(!(0..l.b.len()).any(|i| l.check_row(i)));
    l.reduce_infinity_norm();
    assert_eq!(determinant(&l.b).abs(), det);
    assert!((0..l.b.len()).any(|i| l.check_row(i)));
    assert_eq!(
        l.b[0],
        vec![0, 5, 5, 5]
            .into_iter()
            .map(Integer::from)
            .collect::<Vec<_>>()
    );
}

#[test]
fn bkz_improves_a_l2_reduced_basis() {
    let mut l = reducer(vec![
        vec![1, 11, -18, -15, -8],
        vec![19, -5, 17, 13, -11],
        vec![8, 13, -16, 6, -13],
        vec![15, 14, 0, 11, -10],
        vec![-19, -3, 18, -9, -17],
    ]);
    let det = determinant(&l.b).abs();
    l.reduce();
    assert_eq!(dot(&l.b[0], &l.b[0]), 340);
    l.reduce_bkz(BkzOptions {
        block_size: 5,
        max_tours: 4,
    });
    assert_eq!(dot(&l.b[0], &l.b[0]), 337);
    assert_eq!(determinant(&l.b).abs(), det);
}

fn hash_parameters(pairs: usize, length: usize, seed: u64, palindrome: bool) -> Parameters {
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
        timeout: 10.,
    }
}

fn verify_collision(a: &str, b: &str, p: &Parameters) {
    assert_ne!(a, b);
    assert_eq!(a.len(), p.length);
    assert_eq!(b.len(), p.length);
    assert!(
        a.bytes()
            .chain(b.bytes())
            .all(|c| (b'a'..b'a' + p.sigma as u8).contains(&c))
    );
    if p.palindrome {
        assert_eq!(a.chars().rev().collect::<String>(), b);
    }
    for (m, q) in p.modulo.iter().zip(&p.base) {
        let hash = |s: &str| {
            s.bytes()
                .rev()
                .fold(Integer::from(0), |h, c| (h * q + Integer::from(c)) % m)
        };
        assert_eq!(hash(a), hash(b));
    }
}

#[test]
fn infinity_finds_collisions_missed_by_bkz() {
    for (pairs, length, seed, reverse) in [(2, 11, 44, false), (3, 16, 6, false), (3, 32, 24, true)]
    {
        let p = hash_parameters(pairs, length, seed, reverse);
        let options = BkzOptions {
            block_size: 20,
            max_tours: 4,
        };
        assert!(matches!(
            anti_hash_with_bkz(p.clone(), options),
            AntiResult::NotFound(..)
        ));
        match anti_hash_with_infinity_norm(p.clone(), options) {
            AntiResult::Ok(_, a, b) => verify_collision(&a, &b, &p),
            _ => panic!("infinity reduction did not find collision"),
        }
    }
}

#[test]
fn expired_deadline_leaves_infinity_basis_unchanged() {
    let mut l = reducer(vec![
        vec![0, 6, 0, -2],
        vec![0, -1, 5, 7],
        vec![0, 0, 9, 1],
        vec![101, 0, 0, 0],
    ]);
    l.residue_columns = 1;
    l.sigma = 6;
    l.timeout = -1.;
    let before = l.b.clone();
    l.reduce_infinity_norm();
    assert_eq!(l.b, before);
}

#[test]
fn bkz_finds_collisions_missed_by_l2() {
    for (pairs, length, seed) in [(2, 11, 39), (3, 16, 26)] {
        let p = hash_parameters(pairs, length, seed, false);
        assert!(matches!(
            anti_hash_with_bkz(
                p.clone(),
                BkzOptions {
                    block_size: 2,
                    max_tours: 4
                }
            ),
            AntiResult::NotFound(..)
        ));
        match anti_hash_with_bkz(
            p.clone(),
            BkzOptions {
                block_size: 20,
                max_tours: 4,
            },
        ) {
            AntiResult::Ok(_, a, b) => verify_collision(&a, &b, &p),
            _ => panic!("BKZ did not find regression collision"),
        }
    }
}

#[test]
fn bkz_preserves_reverse_constraint() {
    let p = hash_parameters(3, 32, 31, true);
    assert!(matches!(
        anti_hash_with_bkz(
            p.clone(),
            BkzOptions {
                block_size: 2,
                max_tours: 4
            }
        ),
        AntiResult::NotFound(..)
    ));
    match anti_hash_with_bkz(
        p.clone(),
        BkzOptions {
            block_size: 20,
            max_tours: 4,
        },
    ) {
        AntiResult::Ok(_, a, b) => verify_collision(&a, &b, &p),
        _ => panic!("no reverse collision"),
    }
}

#[test]
fn large_norms_do_not_require_f64_conversion() {
    let mut l = reducer(vec![vec![4, 1, 0], vec![1, 3, 1], vec![2, -1, 2]]);
    let scale: Integer = format!("1{}", "0".repeat(400)).parse().unwrap();
    for row in &mut l.b {
        for value in row {
            *value *= &scale;
        }
    }
    assert!(l.refresh_gso());
    let mut enumeration = Enumeration::new(l.int_to_float(&dot(&l.b[0], &l.b[0])));
    l.enumerate_block(
        0,
        2,
        &mut vec![Integer::from(0); 3],
        Float::from(0),
        &mut enumeration,
    );
    let c = enumeration.best.unwrap();
    let v: Vec<Integer> = (0..3)
        .map(|j| (0..3).map(|i| &c[i] * &l.b[i][j]).sum())
        .collect();
    // Independently brute-force the unscaled squared norm.
    let mut exact = i32::MAX;
    for a in -8..=8 {
        for b in -8..=8 {
            for c in -8..=8 {
                if a == 0 && b == 0 && c == 0 {
                    continue;
                }
                let v = [4 * a + b + 2 * c, a + 3 * b - c, b + 2 * c];
                exact = exact.min(v.iter().map(|x| x * x).sum());
            }
        }
    }
    assert_eq!(dot(&v, &v), Integer::from(exact) * &scale * &scale);
}

#[test]
fn exhausted_deadline_stops_enumeration() {
    let mut l = reducer(vec![vec![4, 1], vec![1, 3]]);
    assert!(l.refresh_gso());
    l.timeout = -1.;
    let mut enumeration = Enumeration::new(Float::from(100));
    l.enumerate_block(
        0,
        1,
        &mut vec![Integer::from(0); 2],
        Float::from(0),
        &mut enumeration,
    );
    assert!(enumeration.best.is_none());
    assert_eq!(enumeration.nodes, 0);
}

#[test]
fn insertion_preserves_lattice_and_inserts_combination() {
    for c in [
        vec![2, 3, 5],
        vec![0, -1, 2],
        vec![-3, 2, 0],
        vec![0, 0, -1],
    ] {
        let mut l = reducer(vec![vec![7, 2, 1], vec![3, 5, 0], vec![1, 1, 4]]);
        let det = determinant(&l.b).abs();
        let expected: Vec<Integer> = (0..3)
            .map(|j| (0..3).map(|i| Integer::from(c[i]) * &l.b[i][j]).sum())
            .collect();
        l.insert_block_vector(0, c.into_iter().map(Integer::from).collect());
        assert_eq!(determinant(&l.b).abs(), det);
        assert_eq!(l.b[0], expected);
    }
}

#[test]
fn nonprimitive_insertion_keeps_the_entire_lattice() {
    let mut l = reducer(vec![vec![1, 0, 0], vec![0, 1, 0], vec![0, 0, 1]]);
    l.insert_block_vector(0, vec![2, 4, 6].into_iter().map(Integer::from).collect());
    assert_eq!(
        l.b[0],
        vec![1, 2, 3]
            .into_iter()
            .map(Integer::from)
            .collect::<Vec<_>>()
    );
    assert_eq!(determinant(&l.b).abs(), 1);
}

#[test]
fn projected_enumeration_agrees_with_exhaustive_search() {
    let mut l = reducer(vec![vec![4, 1, 0], vec![1, 3, 1], vec![2, -1, 2]]);
    assert!(l.refresh_gso());
    for start in 0..2 {
        let mut enumeration = Enumeration::new(Float::from(1000));
        l.enumerate_block(
            start,
            2,
            &mut vec![Integer::from(0); 3 - start],
            Float::from(0),
            &mut enumeration,
        );
        let mut exact = f64::INFINITY;
        // Independent projection onto the orthogonal complement of row 0.
        for a in -8..=8 {
            for b in -8..=8 {
                for c in -8..=8 {
                    if (start == 1 && a != 0) || (a == 0 && b == 0 && c == 0) {
                        continue;
                    }
                    let v: Vec<f64> = (0..3)
                        .map(|j| {
                            [a, b, c]
                                .iter()
                                .enumerate()
                                .map(|(i, x)| *x as f64 * i32::try_from(&l.b[i][j]).unwrap() as f64)
                                .sum()
                        })
                        .collect();
                    let mut norm: f64 = v.iter().map(|x| x * x).sum();
                    if start == 1 {
                        let b0 = [4., 1., 0.];
                        let dot: f64 = v.iter().zip(b0).map(|(x, y)| x * y).sum();
                        norm -= dot * dot / 17.;
                    }
                    exact = exact.min(norm);
                }
            }
        }
        let actual: f64 = enumeration.radius.rounding_into(RoundingMode::Nearest).0;
        assert!((actual - exact).abs() < 1e-10, "{actual} != {exact}");
        assert!(enumeration.best.is_some());
    }
}
