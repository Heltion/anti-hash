use super::*;

pub(super) struct Enumeration {
    pub radius: Float,
    pub best: Option<Vec<Integer>>,
    pub nodes: usize,
}
impl Enumeration {
    pub fn new(radius: Float) -> Self {
        Self {
            radius,
            best: None,
            nodes: 0,
        }
    }
}

impl Reducer {
    /// Recompute GSO after an integer basis change. All enumeration arithmetic
    /// uses the same arbitrary precision as L², including the search radius.
    pub(super) fn refresh_gso(&mut self) -> bool {
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
            .all(|(i, row)| row[i].is_finite() && row[i] > 0)
    }

    pub(crate) fn reduce_bkz(&mut self, options: BkzOptions) {
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
                let mut enumeration =
                    Enumeration::new(self.with_prec(&self.delta * &self.r[start][start]));
                let mut coefficients = vec![Integer::from(0); end - start];
                self.enumerate_block(
                    start,
                    end - 1,
                    &mut coefficients,
                    Float::from(0),
                    &mut enumeration,
                );
                if let Some(coefficients) = enumeration.best {
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
    pub(super) fn enumerate_block(
        &self,
        start: usize,
        level: usize,
        coefficients: &mut [Integer],
        partial: Float,
        enumeration: &mut Enumeration,
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
                enumeration.nodes += 1;
                if enumeration.nodes.is_multiple_of(256) && self.check_time_out() {
                    return;
                }
                if norm < enumeration.radius {
                    inside = true;
                    coefficients[local] = x;
                    if level > start {
                        self.enumerate_block(start, level - 1, coefficients, norm, enumeration);
                    } else if coefficients.iter().any(|x| x != &Integer::from(0)) {
                        enumeration.radius = norm;
                        enumeration.best = Some(coefficients.to_vec());
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
    pub(super) fn insert_block_vector(&mut self, start: usize, mut coefficients: Vec<Integer>) {
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
}
