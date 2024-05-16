use bigdecimal::{num_bigint::BigInt, BigDecimal, FromPrimitive, One, Zero};
use web_sys::js_sys::Date;
const SIGMA: i32 = 26;
pub fn powers(base: &BigInt, modulo: &BigInt, length: usize) -> Vec<BigInt> {
    let mut result = Vec::new();
    result.push(BigInt::one() % modulo);
    for _ in 1..length {
        result.push(result.last().unwrap() * base % modulo);
    }
    result
}

fn dot(a: &Vec<BigInt>, b: &Vec<BigInt>) -> BigInt {
    a.iter().zip(b.iter()).map(|(a, b)| a * b).sum::<BigInt>()
}
struct L2 {
    b: Vec<Vec<BigInt>>,
    r: Vec<Vec<BigDecimal>>,
    mu: Vec<Vec<BigDecimal>>,
    s: Vec<Vec<BigDecimal>>,
    delta: BigDecimal,
    eta: BigDecimal,
    precision: u64,
    timeout: f64,
    start_time: f64,
    n_: usize,
}

impl L2 {
    fn new(
        b: Vec<Vec<BigInt>>,
        delta: BigDecimal,
        eta: BigDecimal,
        precision: u64,
        timeout: f64,
        n_: usize,
    ) -> Self {
        let n = b.len();
        let delta = (delta + 1) / 2;
        let eta = (eta * 2 + 1) / 4;
        let start_time = Date::now() / 1000.;
        Self {
            b,
            r: vec![vec![BigDecimal::zero(); n]; n],
            mu: vec![vec![BigDecimal::zero(); n]; n],
            s: vec![vec![BigDecimal::zero(); n]; n],
            delta,
            eta,
            precision,
            timeout,
            start_time,
            n_,
        }
    }
    fn cfa(&mut self, i: usize) {
        for j in 0..i {
            self.r[i][j] = dot(&self.b[i], &self.b[j]).into();
            self.r[i][j] = self.r[i][j].with_prec(self.precision);
            for k in 0..j {
                let mul = &self.mu[j][k] * &self.r[i][k];
                self.r[i][j] -= mul;
                self.r[i][j] = self.r[i][j].with_prec(self.precision);
            }
            self.mu[i][j] = &self.r[i][j] / &self.r[j][j];
            self.s[i][0] = dot(&self.b[i], &self.b[i]).into();
            self.s[i][0] = self.s[i][0].with_prec(self.precision);
            for j in 1..i + 1 {
                self.s[i][j] = &self.s[i][j - 1] - &self.mu[i][j - 1] * &self.r[i][j - 1];
                self.s[i][j] = self.s[i][j].with_prec(self.precision)
            }
            self.r[i][i] = self.s[i][i].clone();
        }
    }
    fn reduce_row(&mut self, k: usize) {
        let n = self.b.len();
        loop {
            self.cfa(k);

            if self.mu[k].iter().take(k).map(|v| v.abs()).max().unwrap() <= self.eta
                || self.check_row(k)
                || self.check_time_out()
            {
                break;
            }
            for i in (0..k).rev() {
                let x = self.mu[k][i].round(0).into_bigint_and_exponent().0;
                for j in 0..i {
                    let mul = &x * &self.mu[i][j];
                    let mul = mul.with_prec(self.precision);
                    self.mu[k][j] -= mul;
                    self.mu[k][j] = self.mu[k][j].with_prec(self.precision);
                }
                for j in 0..n {
                    self.b[k][j] = &self.b[k][j] - &x * &self.b[i][j];
                }
            }
        }
    }
    fn reduce(&mut self) {
        self.r[0][0] = dot(&self.b[0], &self.b[0]).into();
        self.r[0][0] = self.r[0][0].with_prec(self.precision);
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
    fn row_max(&self, i: usize) -> BigInt {
        self.b[i][self.n_..]
            .iter()
            .map(|val| {
                if val >= &BigInt::ZERO {
                    val.clone()
                } else {
                    -val
                }
            })
            .max()
            .unwrap()
    }
    fn check_row(&self, i: usize) -> bool {
        if self.b[i][..self.n_].iter().any(|val| val != &BigInt::ZERO) {
            return false;
        }
        if self.row_max(i) >= BigInt::from_i32(SIGMA).unwrap() {
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
    pub modulo: Vec<BigInt>,
    pub base: Vec<BigInt>,

    pub lambda: BigInt,
    pub delta: BigDecimal,
    pub eta: BigDecimal,
    pub precision: u64,
    pub timeout: f64,
}

pub enum AntiResult {
    NotFound(f64, Option<Vec<BigInt>>),
    TimeOut(Option<Vec<BigInt>>),
    Ok(f64, String, String),
    Unknown,
}

fn check(a: &String, b: &String, modulo: Vec<BigInt>, base: Vec<BigInt>) -> bool {
    let n = modulo.len();
    let length = 0;
    let a = a
        .chars()
        .map(|c| BigInt::from_u8(c as u8).unwrap())
        .collect::<Vec<_>>();
    let b = b
        .chars()
        .map(|c| BigInt::from_u8(c as u8).unwrap())
        .collect::<Vec<_>>();
    for i in 0..n {
        let pow = powers(&base[i], &modulo[i], length);
        let mut ha = BigInt::ZERO;
        let mut hb = BigInt::ZERO;
        for j in 0..length {
            ha += &pow[j] * &a[j];
            hb += &pow[j] * &b[j];
        }
        if ha != hb {
            return false;
        }
    }
    true
}

pub fn anti_hash(parameters: Parameters) -> AntiResult {
    let Parameters {
        length,
        modulo,
        base,
        lambda,
        delta,
        eta,
        precision,
        timeout,
    } = parameters;
    let n = modulo.len();
    let mut b = vec![vec![BigInt::ZERO; length + n]; length + n];
    for i in 0..n {
        powers(&base[i], &modulo[i], length)
            .into_iter()
            .enumerate()
            .for_each(|(j, val)| b[j][i] = val * &lambda);
        b[length + i][i] = &modulo[i] * &lambda;
    }
    for i in 0..length {
        b[i][n + i] = BigInt::one();
    }
    let mut l2 = L2::new(b, delta, eta, precision, timeout, n);
    l2.reduce();
    let mut best = None;
    let mut best_vec = None;
    for row in &l2.b {
        if row[..n].iter().any(|val| val != &BigInt::ZERO) {
            continue;
        }
        let cur = row[n..]
            .iter()
            .map(|val| {
                if val >= &BigInt::ZERO {
                    val.clone()
                } else {
                    -val
                }
            })
            .max()
            .unwrap();
        if cur < BigInt::from_i32(SIGMA).unwrap() {
            let mut a = String::new();
            let mut b = String::new();
            for i in 0..length {
                let diff: i32 = row[n + i].clone().try_into().unwrap();
                if diff >= 0 {
                    a.push('a');
                    b.push(('a' as u8 + diff as u8) as char);
                } else {
                    a.push(('a' as u8 + (-diff) as u8) as char);
                    b.push('a');
                }
            }
            if !check(&a, &b, modulo, base) {
                return AntiResult::Unknown;
            }
            return AntiResult::Ok(l2.runtime(), a, b);
        }
        if best.is_none() || best.clone().unwrap() > cur {
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
