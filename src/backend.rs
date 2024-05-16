use bigdecimal::{num_bigint::BigInt, BigDecimal, FromPrimitive, One, Zero};

const MAXIMUM_LENGTH: usize = 256;
const SIGMA: usize = 26;
const PRICISION: u64 = 10;
const LAMBDA: i32 = 100000;
const DELTA: f64 = 0.99;
const ETA: f64 = 0.51;
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
}

impl L2 {
    fn new(b: Vec<Vec<BigInt>>) -> Self {
        let n = b.len();
        Self {
            b,
            r: vec![vec![BigDecimal::zero(); n]; n],
            mu: vec![vec![BigDecimal::zero(); n]; n],
            s: vec![vec![BigDecimal::zero(); n]; n],
        }
    }
    fn cfa(&mut self, i: usize) {
        for j in 0..i {
            self.r[i][j] = dot(&self.b[i], &self.b[j]).into();
            self.r[i][j] = self.r[i][j].with_prec(PRICISION);
            for k in 0..j {
                let mul = &self.mu[j][k] * &self.r[i][k];
                self.r[i][j] -= mul;
                self.r[i][j] = self.r[i][j].with_prec(PRICISION);
            }
            self.mu[i][j] = &self.r[i][j] / &self.r[j][j];
            self.s[i][0] = dot(&self.b[i], &self.b[i]).into();
            self.s[i][0] = self.s[i][0].with_prec(PRICISION);
            for j in 1..i + 1 {
                self.s[i][j] = &self.s[i][j - 1] - &self.mu[i][j - 1] * &self.r[i][j - 1];
                self.s[i][j] = self.s[i][j].with_prec(PRICISION)
            }
            self.r[i][i] = self.s[i][i].clone();
        }
    }
    fn reduce_row(&mut self, k: usize) {
        let eta = (BigDecimal::from_f64(ETA).unwrap() + BigDecimal::from_f64(0.5).unwrap())
            / BigDecimal::from_i32(2).unwrap();
        let eta = eta.with_prec(PRICISION);
        let n = self.b.len();
        loop {
            self.cfa(k);

            if self.mu[k].iter().take(k).map(|v| v.abs()).max().unwrap() <= eta {
                break;
            }
            for i in (0..k).rev() {
                let x = self.mu[k][i].round(0).into_bigint_and_exponent().0;
                for j in 0..i {
                    let mul = &x * &self.mu[i][j];
                    let mul = mul.with_prec(PRICISION);
                    self.mu[k][j] -= mul;
                    self.mu[k][j] = self.mu[k][j].with_prec(PRICISION);
                }
                for j in 0..n {
                    self.b[k][j] = &self.b[k][j] - &x * &self.b[i][j];
                }
            }
        }
    }
    fn reduce(&mut self) {
        let delta = (BigDecimal::from_f64(DELTA).unwrap() + BigDecimal::one())
            / BigDecimal::from_i32(2).unwrap();
        let delta = delta.with_prec(PRICISION);
        self.r[0][0] = dot(&self.b[0], &self.b[0]).into();
        self.r[0][0] = self.r[0][0].with_prec(PRICISION);
        let mut k = 1;
        let n = self.b.len();
        while k < n {
            self.reduce_row(k);
            let k_ = k;
            while k >= 1 && &delta * &self.r[k - 1][k - 1] > self.s[k_][k - 1] {
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

fn l2(b: Vec<Vec<BigInt>>) -> Vec<Vec<BigInt>> {
    let mut l2 = L2::new(b);
    l2.show_b(l2.b.len());
    l2.reduce();
    l2.show_b(l2.b.len());
    l2.b
}

pub fn anti_hash_with_length(
    modulo_base: &Vec<(i64, i64)>,
    length: usize,
) -> Option<(String, String)> {
    let n = modulo_base.len();
    let mut b = vec![vec![BigInt::ZERO; length + n]; length + n];
    for i in 0..n {
        let modulo = BigInt::from_i64(modulo_base[i].0).unwrap();
        let base = BigInt::from_i64(modulo_base[i].1).unwrap();
        powers(&base, &modulo, length)
            .into_iter()
            .enumerate()
            .for_each(|(j, val)| b[j][i] = val * LAMBDA);
        b[length + i][i] = modulo * LAMBDA;
    }
    for i in 0..length {
        b[i][n + i] = BigInt::one();
    }
    let b = l2(b);
    None
}

pub fn anti_hash(modulo_base: Vec<(i64, i64)>) -> Result<(String, String), String> {
    let mut length = 1;
    while length <= MAXIMUM_LENGTH {
        if let Some(result) = anti_hash_with_length(&modulo_base, length) {
            return Ok(result);
        }
        length *= 2;
    }
    Err("Not found.".to_string())
}

#[cfg(test)]
mod tests {}
