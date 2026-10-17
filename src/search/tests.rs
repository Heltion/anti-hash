use super::*;

fn parameters() -> Parameters {
    Parameters {
        length: 8,
        modulo: vec![Integer::from(101)],
        base: vec![Integer::from(3)],
        lambda: Integer::from(100_000),
        delta: Float::from(0.99),
        eta: Float::from(0.51),
        precision: 40,
        palindrome: false,
        sigma: 26,
        timeout: 2.0,
    }
}

#[test]
fn invalid_parameters_return_errors_before_reduction() {
    let invalid: &[fn(&mut Parameters)] = &[
        |p| p.length = 0,
        |p| p.base.clear(),
        |p| {
            p.modulo.clear();
            p.base.clear();
        },
        |p| p.modulo[0] = Integer::from(0),
        |p| p.modulo[0] = Integer::from(-1),
        |p| p.base[0] = Integer::from(-1),
        |p| p.base[0] = p.modulo[0].clone(),
        |p| p.lambda = Integer::from(0),
        |p| p.sigma = 1,
        |p| p.sigma = 27,
        |p| p.precision = 0,
        |p| p.precision = 101,
        |p| p.timeout = 0.0,
        |p| p.timeout = f64::INFINITY,
        |p| p.timeout = f64::NAN,
        |p| p.delta = Float::from(f64::NAN),
        |p| p.delta = Float::from(f64::INFINITY),
        |p| p.delta = Float::from(-1),
        |p| p.delta = Float::from(1),
        |p| p.eta = Float::from(0.5),
        |p| p.eta = Float::from(1),
        |p| p.eta = Float::from(f64::NAN),
        |p| p.length = usize::MAX,
    ];
    for change in invalid {
        let mut p = parameters();
        change(&mut p);
        assert!(search(p, SearchOptions::default()).is_err());
    }
    assert!(
        search(
            parameters(),
            SearchOptions {
                bkz: BkzOptions {
                    block_size: 1,
                    max_tours: 4
                },
                infinity_norm: false
            }
        )
        .is_err()
    );
}

fn verified(p: Parameters) {
    let AntiResult::Ok(_, a, b) = search(p.clone(), SearchOptions::default()).unwrap() else {
        panic!("expected a collision");
    };
    assert_ne!(a, b);
    assert_eq!(a.len(), p.length);
    assert_eq!(b.len(), p.length);
    assert!(collision(&a, &b, &p));
    if p.palindrome {
        assert_eq!(a.chars().rev().collect::<String>(), b);
    }
}

#[test]
fn degenerate_hashes_and_reverse_lengths_are_supported() {
    let mut p = parameters();
    p.length = 1;
    p.modulo[0] = Integer::from(1);
    p.base[0] = Integer::from(0);
    verified(p.clone());
    p.palindrome = true;
    assert!(matches!(
        search(p, SearchOptions::default()).unwrap(),
        AntiResult::NotFound(_, None)
    ));
    for length in [2, 3, 8, 9] {
        let mut p = parameters();
        p.length = length;
        p.base[0] = Integer::from(1);
        p.palindrome = true;
        verified(p);
    }
    let mut p = parameters();
    p.length = 2;
    p.base[0] = Integer::from(0);
    verified(p);
}

#[test]
fn large_moduli_and_lambdas_keep_exact_integer_values() {
    let mut p = parameters();
    p.modulo[0] = "18446744073709551616000000000000000000001".parse().unwrap();
    p.lambda = "10000000000000000000000000000000000000000".parse().unwrap();
    p.base[0] = Integer::from(1);
    verified(p);
}

#[test]
fn power_sequence_handles_empty_and_singleton_inputs() {
    let base = Integer::from(3);
    let modulo = Integer::from(7);
    assert_eq!(powers(&base, &modulo, 0).count(), 0);
    assert_eq!(
        powers(&base, &modulo, 4).collect::<Vec<_>>(),
        [1, 3, 2, 6].map(Integer::from)
    );
}
