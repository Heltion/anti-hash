use malachite::base::num::arithmetic::traits::Abs;
use malachite::base::num::conversion::traits::RoundingInto;
use malachite::base::rounding_modes::RoundingMode;
use malachite::{Integer, Rational};
use malachite_float::Float;
use std::convert::TryFrom;
use web_sys::js_sys::Date;

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
            start_time: Date::now() / 1000.,
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
        Date::now() / 1000. - self.start_time
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
    l2.reduce();
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
    if parameters.palindrome {
        return anti_palindrome_hash(parameters);
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
    l2.reduce();
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
mod tests {}
