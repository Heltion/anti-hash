use crate::{BkzOptions, clock::Clock};
use malachite::base::num::arithmetic::traits::Abs;
use malachite::base::num::conversion::traits::RoundingInto;
use malachite::base::rounding_modes::RoundingMode;
use malachite::{Float, Integer};
mod bkz;
#[cfg(test)]
mod tests;

fn dot(a: &[Integer], b: &[Integer]) -> Integer {
    a.iter().zip(b.iter()).map(|(a, b)| a * b).sum::<Integer>()
}

pub(crate) struct Reducer {
    pub(crate) b: Vec<Vec<Integer>>,
    r: Vec<Vec<Float>>,
    mu: Vec<Vec<Float>>,
    s: Vec<Vec<Float>>,
    delta: Float,
    eta: Float,
    precision: u64,
    sigma: usize,
    pub(crate) allowed_differences: Option<Vec<bool>>,
    timeout: f64,
    clock: Clock,
    residue_columns: usize,
}

impl Reducer {
    pub(crate) fn new(
        b: Vec<Vec<Integer>>,
        delta: Float,
        eta: Float,
        precision: u64,
        sigma: usize,
        timeout: f64,
        residue_columns: usize,
    ) -> Self {
        let n = b.len();
        // Converting decimal precision to bits roughly
        let prec_bits = (precision as f64 * std::f64::consts::LOG2_10 + 10.0) as u64;

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
            allowed_differences: None,
            timeout,
            clock: Clock::start(),
            residue_columns,
        }
    }

    // Helper to apply precision
    fn with_prec(&self, mut val: Float) -> Float {
        val.set_prec(self.precision);
        val
    }

    // Helper to convert Integer to Float with precision
    fn int_to_float(&self, val: &Integer) -> Float {
        Float::from_integer_prec_ref(val, self.precision).0
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
        }
        self.s[i][0] = self.int_to_float(&dot(&self.b[i], &self.b[i]));
        for j in 1..=i {
            self.s[i][j] =
                self.with_prec(&self.s[i][j - 1] - &self.mu[i][j - 1] * &self.r[i][j - 1]);
        }
        self.r[i][i] = self.s[i][i].clone();
    }

    fn reduce_row(&mut self, k: usize) {
        loop {
            self.cfa(k);

            // max finding
            let max_abs_mu = self.mu[k].iter().take(k).fold(Float::from(0.0), |max, v| {
                let abs_v = v.abs();
                if abs_v > max { abs_v } else { max }
            });

            if max_abs_mu <= self.eta || self.check_row(k) || self.check_time_out() {
                break;
            }

            for i in (0..k).rev() {
                let x: Integer = self.mu[k][i].clone().rounding_into(RoundingMode::Nearest).0;

                if x == 0 {
                    continue;
                }
                let x_float = self.int_to_float(&x);
                for j in 0..i {
                    let mul_float = &x_float * &self.mu[i][j];
                    let diff = &self.mu[k][j] - self.with_prec(mul_float);
                    self.mu[k][j] = self.with_prec(diff);
                }
                let (before, after) = self.b.split_at_mut(k);
                for (value, source) in after[0].iter_mut().zip(&before[i]) {
                    *value -= &x * source;
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
    /// Optimize actual character differences on the zero-residue sublattice.
    /// Each accepted b_i <- b_i + t*b_j is unimodular and strictly decreases
    /// that row's infinity norm. Sweep and timeout budgets bound the work;
    /// failing to find a collision is not a nonexistence proof.
    pub(crate) fn reduce_infinity_norm(&mut self) {
        use crate::infinity_norm::{bounded_step, improving_step};
        let mut rows: Vec<usize> = (0..self.b.len())
            .filter(|&i| {
                self.b[i][..self.residue_columns]
                    .iter()
                    .all(|x| x == &Integer::from(0))
            })
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
                    let v = &self.b[i][self.residue_columns..];
                    let w = &self.b[j][self.residue_columns..];
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
        self.b[i][self.residue_columns..]
            .iter()
            .map(|val| val.abs())
            .max()
            .unwrap()
    }
    fn check_row(&self, i: usize) -> bool {
        if self.b[i][..self.residue_columns]
            .iter()
            .any(|val| val != &Integer::from(0))
        {
            return false;
        }
        if self.row_max(i) >= self.sigma {
            return false;
        }
        self.allowed_differences.as_ref().is_none_or(|allowed| {
            self.b[i][self.residue_columns..].iter().all(|x| {
                usize::try_from(&x.abs())
                    .ok()
                    .is_some_and(|d| allowed.get(d) == Some(&true))
            })
        })
    }
    pub(crate) fn runtime(&self) -> f64 {
        self.clock.elapsed()
    }
    pub(crate) fn check_time_out(&self) -> bool {
        self.runtime() > self.timeout
    }
}
