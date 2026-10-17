use malachite::base::num::arithmetic::traits::Abs;
use malachite::base::num::conversion::traits::RoundingInto;
use malachite::base::rounding_modes::RoundingMode;
use malachite::{Integer, Rational};
use malachite_float::Float;
use std::convert::TryFrom;
#[cfg(target_arch = "wasm32")]
use web_sys::js_sys::Date;

#[cfg(target_arch = "wasm32")]
fn now() -> f64 {
    Date::now() / 1000.
}
#[cfg(not(target_arch = "wasm32"))]
fn now() -> f64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

pub fn powers(base: &Integer, modulo: &Integer, length: usize) -> Vec<Integer> {
    let mut result = Vec::new();
    result.push(Integer::from(1) % modulo);
    for _ in 1..length {
        result.push(result.last().unwrap() * base % modulo);
    }
    result
}

fn dot(a: &Vec<Integer>, b: &Vec<Integer>) -> Integer {
    a.iter().zip(b.iter()).map(|(a, b)| a * b).sum::<Integer>()
}

struct L2 {
    b: Vec<Vec<Integer>>,
    r: Vec<Vec<Float>>,
    mu: Vec<Vec<Float>>,
    s: Vec<Vec<Float>>,
    delta: Float,
    eta: Float,
    precision: u64,
    sigma: usize,
    timeout: f64,
    start_time: f64,
    n_: usize,
}

impl L2 {
    fn new(
        b: Vec<Vec<Integer>>,
        delta: Float,
        eta: Float,
        precision: u64,
        sigma: usize,
        timeout: f64,
        n_: usize,
    ) -> Self {
        let n = b.len();
        // Converting decimal precision to bits roughly
        let prec_bits = (precision as f64 * 3.3219 + 10.0) as u64;

        // Initialize with correct precision
        let zero = |prec: u64| -> Float {
            let mut f = Float::from(0.0);
            f.set_prec(prec);
            f
        };

        Self {
            b,
            r: vec![vec![zero(prec_bits); n]; n],
            mu: vec![vec![zero(prec_bits); n]; n],
            s: vec![vec![zero(prec_bits); n]; n],
            delta: (delta + Float::one_prec(prec_bits)) / Float::from(2.0),
            eta: (eta * Float::from(2.0) + Float::one_prec(prec_bits)) / Float::from(4.0),
            precision: prec_bits,
            sigma,
            timeout,
            start_time: now(),
            n_,
        }
    }

    // Helper to apply precision
    fn with_prec(&self, mut val: Float) -> Float {
        val.set_prec(self.precision);
        val
    }

    // Helper to convert Integer to Float with precision
    fn int_to_float(&self, val: &Integer) -> Float {
        let r = Rational::from(val.clone());
        Float::from_rational_prec(r, self.precision).0
    }

    fn cfa(&mut self, i: usize) {
        for j in 0..i {
            let dot_val = dot(&self.b[i], &self.b[j]);
            self.r[i][j] = self.int_to_float(&dot_val);

            for k in 0..j {
                let mul = &self.mu[j][k] * &self.r[i][k];
                let diff = &self.r[i][j] - mul;
                self.r[i][j] = self.with_prec(diff);
            }

            let div_res = &self.r[i][j] / &self.r[j][j];
            self.mu[i][j] = self.with_prec(div_res);

            let dot_val_i = dot(&self.b[i], &self.b[i]);
            self.s[i][0] = self.int_to_float(&dot_val_i);

            for j in 1..i + 1 {
                let term = &self.mu[i][j - 1] * &self.r[i][j - 1];
                let diff = &self.s[i][j - 1] - term;
                self.s[i][j] = self.with_prec(diff);
            }
            self.r[i][i] = self.s[i][i].clone();
        }
    }

    fn reduce_row(&mut self, k: usize) {
        let n = self.b.len();
        loop {
            self.cfa(k);

            // max finding
            let max_abs_mu = self.mu[k].iter().take(k).fold(Float::from(0.0), |max, v| {
                let abs_v = v.abs();
                if abs_v > max {
                    abs_v
                } else {
                    max
                }
            });

            if max_abs_mu <= self.eta || self.check_row(k) || self.check_time_out() {
                break;
            }

            for i in (0..k).rev() {
                let x: Integer = self.mu[k][i].clone().rounding_into(RoundingMode::Nearest).0;

                for j in 0..i {
                    let mul_float = self.int_to_float(&x) * &self.mu[i][j];
                    let diff = &self.mu[k][j] - self.with_prec(mul_float);
                    self.mu[k][j] = self.with_prec(diff);
                }
                for j in 0..n {
                    let val_i = self.b[i][j].clone();
                    self.b[k][j] -= &x * &val_i;
                }
            }
        }
    }

    fn reduce(&mut self) {
        let dot_val = dot(&self.b[0], &self.b[0]);
        self.r[0][0] = self.int_to_float(&dot_val);

        let mut k = 1;
        let n = self.b.len();
        while k < n {
            self.reduce_row(k);
            if self.check_row(k) || self.check_time_out() {
                return;
            }
            let k_ = k;
            while k >= 1 && &self.delta * &self.r[k - 1][k - 1] > self.s[k_][k - 1] {
                k -= 1
            }
            if k_ != k {
                for i in 0..k {
                    self.mu[k][i] = self.mu[k_][i].clone();
                    self.r[k][i] = self.r[k_][i].clone();
                }
                self.r[k][k] = self.s[k_][k].clone();
                self.b[k..k_ + 1].rotate_right(1);
            }
            k += 1;
        }
    }
    /// Recompute GSO after an integer basis change. All enumeration arithmetic
    /// uses the same arbitrary precision as L², including the search radius.
    fn refresh_gso(&mut self) -> bool {
        self.r[0][0] = self.int_to_float(&dot(&self.b[0], &self.b[0]));
        for i in 1..self.b.len() {
            if self.check_time_out() {
                return false;
            }
            self.cfa(i);
        }
        self.r
            .iter()
            .enumerate()
            .all(|(i, row)| row[i].is_finite() && row[i] > Float::from(0))
    }

    fn reduce_bkz(&mut self, options: BkzOptions) {
        self.reduce();
        if options.block_size <= 2 {
            return;
        }
        let mut gso_valid = false;
        for _ in 0..options.max_tours {
            let mut changed = false;
            for start in 0..self.b.len().saturating_sub(1) {
                if self.check_time_out() || (0..self.b.len()).any(|i| self.check_row(i)) {
                    return;
                }
                if !gso_valid {
                    if !self.refresh_gso() {
                        return;
                    }
                    gso_valid = true;
                }
                let end = (start + options.block_size.min(self.b.len())).min(self.b.len());
                let mut radius = self.with_prec(&self.delta * &self.r[start][start]);
                let mut coefficients = vec![Integer::from(0); end - start];
                let mut best = None;
                let mut nodes = 0;
                self.enumerate_block(
                    start,
                    end - 1,
                    &mut coefficients,
                    Float::from(0),
                    &mut radius,
                    &mut best,
                    &mut nodes,
                );
                if let Some(coefficients) = best {
                    self.insert_block_vector(start, coefficients);
                    self.reduce();
                    gso_valid = false;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// Exhaustive Schnorr–Euchner enumeration in the projected block. The
    /// timeout may interrupt it, so a bounded run is a search, not a certificate.
    fn enumerate_block(
        &self,
        start: usize,
        level: usize,
        coefficients: &mut [Integer],
        partial: Float,
        radius: &mut Float,
        best: &mut Option<Vec<Integer>>,
        nodes: &mut usize,
    ) {
        if self.check_time_out() {
            return;
        }
        let local = level - start;
        let mut center = Float::from(0);
        for j in level + 1..start + coefficients.len() {
            center -= self.int_to_float(&coefficients[j - start]) * &self.mu[j][level];
        }
        let nearest: Integer = center.clone().rounding_into(RoundingMode::Nearest).0;
        // Visit the closest integer, then both sides at increasing distance.
        let mut offset = Integer::from(0);
        loop {
            let mut inside = false;
            for sign in [1i32, -1] {
                if sign == -1 && offset == 0 {
                    continue;
                }
                let x = &nearest + &offset * Integer::from(sign);
                let distance = self.int_to_float(&x) - &center;
                let norm = self.with_prec(&partial + &distance * &distance * &self.r[level][level]);
                *nodes += 1;
                if *nodes % 256 == 0 && self.check_time_out() {
                    return;
                }
                if norm < *radius {
                    inside = true;
                    coefficients[local] = x;
                    if level > start {
                        self.enumerate_block(
                            start,
                            level - 1,
                            coefficients,
                            norm,
                            radius,
                            best,
                            nodes,
                        );
                    } else if coefficients.iter().any(|x| x != &Integer::from(0)) {
                        *radius = norm;
                        *best = Some(coefficients.to_vec());
                    }
                }
            }
            if !inside {
                break;
            }
            offset += Integer::from(1);
        }
    }

    /// Euclidean column operations on the coefficients, with inverse integer
    /// row operations on the basis. Every step is unimodular (add or swap),
    /// so the lattice is preserved even if enumeration was interrupted.
    fn insert_block_vector(&mut self, start: usize, mut coefficients: Vec<Integer>) {
        for j in 1..coefficients.len() {
            while coefficients[j] != 0 {
                let q = &coefficients[0] / &coefficients[j];
                let cj = coefficients[j].clone();
                coefficients[0] -= &q * cj;
                let pivot = self.b[start].clone();
                for (value, pivot) in self.b[start + j].iter_mut().zip(pivot) {
                    *value += &q * pivot;
                }
                self.b.swap(start, start + j);
                coefficients.swap(0, j);
            }
        }
        if coefficients[0] < 0 {
            for value in &mut self.b[start] {
                *value = -value.clone();
            }
        }
    }

    /// Optimize actual character differences on the zero-residue sublattice.
    /// Each accepted b_i <- b_i + t*b_j is unimodular and strictly decreases
    /// that row's infinity norm. Sweep and timeout budgets bound the work;
    /// failing to find a collision is not a nonexistence proof.
    fn reduce_infinity_norm(&mut self) {
        use crate::infinity_norm::{bounded_step, improving_step};
        let mut rows: Vec<usize> = (0..self.b.len())
            .filter(|&i| self.b[i][..self.n_].iter().all(|x| x == &Integer::from(0)))
            .collect();
        if rows.iter().any(|&i| self.check_row(i)) || self.check_time_out() {
            return;
        }
        rows.sort_by_key(|&i| self.row_max(i));
        let target = Integer::from(self.sigma) - Integer::from(1);
        for _ in 0..2 {
            let mut changed = false;
            for &i in &rows {
                for &j in &rows {
                    if i == j {
                        continue;
                    }
                    if self.check_time_out() {
                        return;
                    }
                    let v = &self.b[i][self.n_..];
                    let w = &self.b[j][self.n_..];
                    let step = bounded_step(v, w, &target).or_else(|| improving_step(v, w));
                    if let Some(step) = step {
                        if step == 0 {
                            continue;
                        }
                        let other = self.b[j].clone();
                        for (x, y) in self.b[i].iter_mut().zip(other) {
                            *x += &step * y;
                        }
                        changed = true;
                        if self.check_row(i) {
                            return;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }

    fn row_max(&self, i: usize) -> Integer {
        self.b[i][self.n_..]
            .iter()
            .map(|val| val.abs())
            .max()
            .unwrap()
    }
    fn check_row(&self, i: usize) -> bool {
        if self.b[i][..self.n_]
            .iter()
            .any(|val| val != &Integer::from(0))
        {
            return false;
        }
        if self.row_max(i) >= Integer::from(self.sigma) {
            return false;
        }
        true
    }
    fn runtime(&self) -> f64 {
        now() - self.start_time
    }
    fn check_time_out(&self) -> bool {
        self.runtime() > self.timeout
    }
    #[allow(unused)]
    fn show_b(&self, k: usize) {
        let n = self.b.len();
        for i in 0..k {
            for j in 0..n {
                print!("{},", self.b[i][j])
            }
            println!();
        }
        println!()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BkzOptions {
    /// 2 disables BKZ and retains the original L²-only search.
    pub block_size: usize,
    /// A finite tour budget; stopping early does not certify BKZ reduction.
    pub max_tours: usize,
}

// BKZ is opt-in: stronger reduction can cost more without finding a collision.
impl Default for BkzOptions {
    fn default() -> Self {
        Self {
            block_size: 2,
            max_tours: 4,
        }
    }
}

#[derive(Clone)]
pub struct Parameters {
    pub length: usize,
    pub modulo: Vec<Integer>,
    pub base: Vec<Integer>,

    pub lambda: Integer,
    pub delta: Float,
    pub eta: Float,
    pub precision: u64,
    pub palindrome: bool,
    pub sigma: usize,

    pub timeout: f64,
}

pub enum AntiResult {
    NotFound(f64, Option<Vec<Integer>>),
    TimeOut(Option<Vec<Integer>>),
    Ok(f64, String, String),
    Unknown,
}

fn check(a: &String, b: &String, modulo: Vec<Integer>, base: Vec<Integer>) -> bool {
    let n = modulo.len();
    let a_vec = a
        .chars()
        .map(|c| Integer::from(c as u8))
        .collect::<Vec<_>>();
    let b_vec = b
        .chars()
        .map(|c| Integer::from(c as u8))
        .collect::<Vec<_>>();

    let len = a_vec.len();

    for i in 0..n {
        let pow = powers(&base[i], &modulo[i], len);
        let mut ha = Integer::from(0);
        let mut hb = Integer::from(0);
        for j in 0..len {
            ha += &pow[j] * &a_vec[j];
            hb += &pow[j] * &b_vec[j];
        }

        if (ha - &hb) % &modulo[i] != Integer::from(0) {
            return false;
        }
    }
    true
}

pub fn anti_palindrome_hash(parameters: Parameters) -> AntiResult {
    anti_palindrome_hash_with_bkz(parameters, BkzOptions::default(), false)
}

fn anti_palindrome_hash_with_bkz(
    parameters: Parameters,
    options: BkzOptions,
    infinity: bool,
) -> AntiResult {
    let Parameters {
        length,
        modulo,
        base,
        lambda,
        delta,
        eta,
        precision,
        timeout,
        palindrome: _,
        sigma,
    } = parameters;
    if length == 1 {
        return AntiResult::NotFound(0., None);
    }
    let n = modulo.len();
    let mut b = vec![vec![Integer::from(0); length / 2 + n]; length / 2 + n];
    for i in 0..n {
        powers(&base[i], &modulo[i], length)
            .into_iter()
            .enumerate()
            .for_each(|(j, val)| {
                if j < length / 2 {
                    b[j][i] += val * &lambda
                } else if length - 1 - j < length / 2 {
                    b[length - 1 - j][i] -= val * &lambda
                }
            });
        b[length / 2 + i][i] = &modulo[i] * &lambda;
    }
    for i in 0..length / 2 {
        b[i][n + i] = Integer::from(1);
    }
    let mut l2 = L2::new(b, delta, eta, precision, sigma, timeout, n);
    l2.reduce_bkz(options);
    if infinity {
        l2.reduce_infinity_norm();
    }
    let mut best = None;
    let mut best_vec = None;
    for row in &l2.b {
        if row[..n].iter().any(|val| val != &Integer::from(0)) {
            continue;
        }
        let cur = row[n..].iter().map(|val| val.abs()).max().unwrap();

        if cur < Integer::from(sigma) {
            let mut a = String::new();
            let mut b = String::new();
            for i in 0..length / 2 {
                let diff_int: i32 = i32::try_from(&row[n + i]).unwrap();
                if diff_int >= 0 {
                    a.push('a');
                    b.push(('a' as u8 + diff_int as u8) as char);
                } else {
                    a.push(('a' as u8 + (-diff_int) as u8) as char);
                    b.push('a');
                }
            }
            let pa = a.clone().chars().into_iter().rev().collect::<String>();
            let pb = b.clone().chars().into_iter().rev().collect::<String>();
            if length % 2 == 1 {
                a.push('a');
                b.push('a');
            }
            a += &pb;
            b += &pa;
            if !check(&a, &b, modulo.clone(), base.clone()) {
                return AntiResult::Unknown;
            }
            return AntiResult::Ok(l2.runtime(), a, b);
        }
        if best.is_none() || best.as_ref().unwrap() > &cur {
            best = Some(cur);
            best_vec = Some(row[n..].iter().map(|val| val.clone()).collect());
        }
    }
    if l2.check_time_out() {
        return AntiResult::TimeOut(best_vec);
    } else {
        AntiResult::NotFound(l2.runtime(), best_vec)
    }
}

pub fn anti_hash(parameters: Parameters) -> AntiResult {
    anti_hash_with_bkz(parameters, BkzOptions::default())
}

/// Run L² first, then bounded projected-block BKZ if no collision was found.
/// Both stages share `parameters.timeout`; failure is not a nonexistence proof.
pub fn anti_hash_with_bkz(parameters: Parameters, options: BkzOptions) -> AntiResult {
    anti_hash_internal(parameters, options, false)
}

/// Add exact local infinity-norm reduction after the configured L²/BKZ search.
/// This preserves baseline successes and shares the original timeout budget.
pub fn anti_hash_with_infinity_norm(parameters: Parameters, options: BkzOptions) -> AntiResult {
    anti_hash_internal(parameters, options, true)
}

fn anti_hash_internal(parameters: Parameters, options: BkzOptions, infinity: bool) -> AntiResult {
    if parameters.palindrome {
        return anti_palindrome_hash_with_bkz(parameters, options, infinity);
    }
    let Parameters {
        length,
        modulo,
        base,
        lambda,
        delta,
        eta,
        precision,
        timeout,
        palindrome: _,
        sigma,
    } = parameters;
    let n = modulo.len();
    let mut b = vec![vec![Integer::from(0); length + n]; length + n];
    for i in 0..n {
        powers(&base[i], &modulo[i], length)
            .into_iter()
            .enumerate()
            .for_each(|(j, val)| b[j][i] = val * &lambda);
        b[length + i][i] = &modulo[i] * &lambda;
    }
    for i in 0..length {
        b[i][n + i] = Integer::from(1);
    }
    let mut l2 = L2::new(b, delta, eta, precision, sigma, timeout, n);
    l2.reduce_bkz(options);
    if infinity {
        l2.reduce_infinity_norm();
    }
    let mut best = None;
    let mut best_vec = None;
    for row in &l2.b {
        if row[..n].iter().any(|val| val != &Integer::from(0)) {
            continue;
        }
        let cur = row[n..].iter().map(|val| val.abs()).max().unwrap();

        if cur < Integer::from(sigma) {
            let mut a = String::new();
            let mut b = String::new();
            for i in 0..length {
                let diff_int: i32 = i32::try_from(&row[n + i]).unwrap();
                if diff_int >= 0 {
                    a.push('a');
                    b.push(('a' as u8 + diff_int as u8) as char);
                } else {
                    a.push(('a' as u8 + (-diff_int) as u8) as char);
                    b.push('a');
                }
            }
            if !check(&a, &b, modulo.clone(), base.clone()) {
                return AntiResult::Unknown;
            }
            return AntiResult::Ok(l2.runtime(), a, b);
        }
        if best.is_none() || best.as_ref().unwrap() > &cur {
            best = Some(cur);
            best_vec = Some(row[n..].iter().map(|val| val.clone()).collect());
        }
    }
    if l2.check_time_out() {
        return AntiResult::TimeOut(best_vec);
    } else {
        AntiResult::NotFound(l2.runtime(), best_vec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reducer(b: Vec<Vec<i32>>) -> L2 {
        L2::new(
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
        l.n_ = 1;
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
        assert!(a
            .bytes()
            .chain(b.bytes())
            .all(|c| (b'a'..b'a' + p.sigma as u8).contains(&c)));
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
        for (pairs, length, seed, reverse) in
            [(2, 11, 44, false), (3, 16, 6, false), (3, 32, 24, true)]
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
        l.n_ = 1;
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
        let mut radius = l.int_to_float(&dot(&l.b[0], &l.b[0]));
        let mut best = None;
        l.enumerate_block(
            0,
            2,
            &mut vec![Integer::from(0); 3],
            Float::from(0),
            &mut radius,
            &mut best,
            &mut 0,
        );
        let c = best.unwrap();
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
        let mut best = None;
        let mut nodes = 0;
        l.enumerate_block(
            0,
            1,
            &mut vec![Integer::from(0); 2],
            Float::from(0),
            &mut Float::from(100),
            &mut best,
            &mut nodes,
        );
        assert!(best.is_none());
        assert_eq!(nodes, 0);
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
            let mut radius = Float::from(1000);
            let mut best = None;
            l.enumerate_block(
                start,
                2,
                &mut vec![Integer::from(0); 3 - start],
                Float::from(0),
                &mut radius,
                &mut best,
                &mut 0,
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
                                    .map(|(i, x)| {
                                        *x as f64 * i32::try_from(&l.b[i][j]).unwrap() as f64
                                    })
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
            let actual: f64 = radius.rounding_into(RoundingMode::Nearest).0;
            assert!((actual - exact).abs() < 1e-10, "{actual} != {exact}");
            assert!(best.is_some());
        }
    }
}
