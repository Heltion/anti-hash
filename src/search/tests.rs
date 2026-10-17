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
        |p| p.sigma = 37,
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
                alphabet: Alphabet::Letters,
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

#[test]
fn all_alphabet_sizes_preserve_literal_ascii_collisions() {
    for alphabet in [Alphabet::Letters, Alphabet::Digits, Alphabet::Both] {
        for size in 2..=alphabet.symbols().len() {
            for palindrome in [false, true] {
                let mut p = parameters();
                p.sigma = size;
                p.base[0] = Integer::from(1);
                p.palindrome = palindrome;
                p.length = 9;
                let options = SearchOptions {
                    alphabet,
                    infinity_norm: true,
                    ..SearchOptions::default()
                };
                let AntiResult::Ok(_, a, b) = search(p.clone(), options).unwrap() else {
                    panic!("missing collision for {alphabet:?}, size {size}");
                };
                let symbols = &alphabet.symbols()[..size];
                assert!(a.bytes().chain(b.bytes()).all(|c| symbols.contains(&c)));
                assert!(collision(&a, &b, &p));
                assert_eq!(a.len(), p.length);
                assert_eq!(b.len(), p.length);
                if palindrome {
                    assert_eq!(a.chars().rev().collect::<String>(), b);
                }
            }
        }
    }
}

#[test]
fn mixed_alphabet_respects_ascii_gaps() {
    let pairs = character_pairs(Alphabet::Both.symbols());
    for difference in -75i32..=75 {
        let mut p = parameters();
        p.length = 1;
        let result = strings(&[Integer::from(difference)], &p, &pairs, b'0');
        let expected = difference.abs() <= 25 || (40..=74).contains(&difference.abs());
        assert_eq!(result.is_some(), expected);
        if let Some((a, b)) = result {
            assert_eq!(
                i32::from(b.as_bytes()[0]) - i32::from(a.as_bytes()[0]),
                difference
            );
        }
    }
    // This needs an actual digit/letter pair with ASCII difference 40.
    let mut p = parameters();
    p.length = 1;
    p.sigma = 36;
    p.modulo[0] = Integer::from(40);
    let options = SearchOptions {
        alphabet: Alphabet::Both,
        ..SearchOptions::default()
    };
    let AntiResult::Ok(_, a, b) = search(p.clone(), options).unwrap() else {
        panic!("missing cross-alphabet collision");
    };
    assert!(collision(&a, &b, &p));
    assert!(a.bytes().chain(b.bytes()).any(|c| c.is_ascii_digit()));
    assert!(a.bytes().chain(b.bytes()).any(|c| c.is_ascii_lowercase()));
}

#[test]
fn size_is_validated_for_selected_alphabet() {
    for (alphabet, size) in [
        (Alphabet::Digits, 11),
        (Alphabet::Letters, 27),
        (Alphabet::Both, 37),
    ] {
        let mut p = parameters();
        p.sigma = size;
        assert!(
            search(
                p,
                SearchOptions {
                    alphabet,
                    ..SearchOptions::default()
                }
            )
            .is_err()
        );
    }
}
