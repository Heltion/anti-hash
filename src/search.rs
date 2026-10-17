use crate::reduction::Reducer;
use malachite::base::num::arithmetic::traits::Abs;
use malachite::{Float, Integer};

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

#[derive(Clone, Debug)]
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

#[derive(Debug)]
pub enum AntiResult {
    NotFound(f64, Option<Vec<Integer>>),
    TimeOut(Option<Vec<Integer>>),
    Ok(f64, String, String),
    Unknown,
}

/// Reduction stages share one timeout. A failed search does not prove that no collision exists.
#[derive(Clone, Copy, Debug, Default)]
pub struct SearchOptions {
    pub bkz: BkzOptions,
    pub infinity_norm: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputError(pub &'static str);
impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for InputError {}

impl Parameters {
    /// Validate before allocating a lattice or performing arbitrary-precision arithmetic.
    pub fn validate(&self) -> Result<(), InputError> {
        let require = |ok, message| if ok { Ok(()) } else { Err(InputError(message)) };
        require(self.length > 0, "length must be positive")?;
        require(
            !self.modulo.is_empty() && self.modulo.len() == self.base.len(),
            "provide matching, nonempty modulo and base lists",
        )?;
        require(
            self.modulo.iter().all(|m| m > &0),
            "moduli must be positive",
        )?;
        require(
            self.base
                .iter()
                .zip(&self.modulo)
                .all(|(b, m)| b >= &0 && b < m),
            "each base must be nonnegative and smaller than its modulus",
        )?;
        require(self.lambda > 0, "lambda must be positive")?;
        require(
            (2..=26).contains(&self.sigma),
            "character set size must be between 2 and 26",
        )?;
        require(
            (1..=100).contains(&self.precision),
            "precision must be between 1 and 100 decimal digits",
        )?;
        require(
            self.timeout.is_finite() && self.timeout > 0.0,
            "timeout must be finite and positive",
        )?;
        require(
            self.delta.is_finite() && self.delta > 0.25 && self.delta < 1,
            "delta must be between 0.25 and 1",
        )?;
        require(
            self.eta.is_finite() && self.eta > 0.5 && &self.eta * &self.eta < self.delta,
            "eta must be greater than 0.5 and its square smaller than delta",
        )?;
        let dimensions = if self.palindrome {
            self.length / 2
        } else {
            self.length
        };
        require(
            dimensions
                .checked_add(self.modulo.len())
                .and_then(|n| n.checked_mul(n))
                .is_some(),
            "lattice dimensions overflow",
        )
    }
}

fn powers<'a>(
    base: &'a Integer,
    modulo: &'a Integer,
    length: usize,
) -> impl Iterator<Item = Integer> + 'a {
    std::iter::successors(Some(Integer::from(1) % modulo), move |previous| {
        Some(previous * base % modulo)
    })
    .take(length)
}

fn lattice(p: &Parameters) -> Vec<Vec<Integer>> {
    let hashes = p.modulo.len();
    let chars = if p.palindrome { p.length / 2 } else { p.length };
    let mut basis = vec![vec![Integer::from(0); hashes + chars]; hashes + chars];
    for (i, (base, modulo)) in p.base.iter().zip(&p.modulo).enumerate() {
        for (j, power) in powers(base, modulo, p.length).enumerate() {
            if !p.palindrome || j < chars {
                basis[j][i] += power * &p.lambda;
            } else if p.length - 1 - j < chars {
                basis[p.length - 1 - j][i] -= power * &p.lambda;
            }
        }
        basis[chars + i][i] = modulo * &p.lambda;
    }
    for (i, row) in basis.iter_mut().take(chars).enumerate() {
        row[hashes + i] = Integer::from(1);
    }
    basis
}

fn strings(differences: &[Integer], p: &Parameters) -> (String, String) {
    let mut a = String::with_capacity(p.length);
    let mut b = String::with_capacity(p.length);
    for difference in differences {
        // The caller checked |difference| < sigma <= 26.
        let difference = i32::try_from(difference).expect("bounded character difference");
        a.push(char::from(b'a' + (-difference).max(0) as u8));
        b.push(char::from(b'a' + difference.max(0) as u8));
    }
    if p.palindrome {
        let suffix_a: String = b.chars().rev().collect();
        let suffix_b: String = a.chars().rev().collect();
        if p.length % 2 == 1 {
            a.push('a');
            b.push('a');
        }
        a.push_str(&suffix_a);
        b.push_str(&suffix_b);
    }
    (a, b)
}

fn collision(a: &str, b: &str, p: &Parameters) -> bool {
    a != b
        && p.base.iter().zip(&p.modulo).all(|(base, modulo)| {
            let hash = |s: &str| {
                s.bytes().rev().fold(Integer::from(0), |h, c| {
                    (h * base + Integer::from(c)) % modulo
                })
            };
            hash(a) == hash(b)
        })
}

/// Checked search API. Every returned collision is independently verified with exact modular arithmetic.
pub fn search(parameters: Parameters, options: SearchOptions) -> Result<AntiResult, InputError> {
    parameters.validate()?;
    if options.bkz.block_size < 2 {
        return Err(InputError("BKZ block size must be at least 2"));
    }
    let p = parameters;
    if p.palindrome && p.length == 1 {
        return Ok(AntiResult::NotFound(0.0, None));
    }
    let basis = lattice(&p);
    let mut reducer = Reducer::new(
        basis,
        p.delta.clone(),
        p.eta.clone(),
        p.precision,
        p.sigma,
        p.timeout,
        p.modulo.len(),
    );
    reducer.reduce_bkz(options.bkz);
    if options.infinity_norm {
        reducer.reduce_infinity_norm();
    }
    let mut best: Option<(Integer, Vec<Integer>)> = None;
    for row in &reducer.b {
        let (residues, differences) = row.split_at(p.modulo.len());
        if residues.iter().any(|x| x != &0) {
            continue;
        }
        let norm = differences
            .iter()
            .map(Abs::abs)
            .max()
            .expect("nonempty character coordinates");
        if norm == 0 {
            continue;
        }
        if norm < p.sigma {
            let (a, b) = strings(differences, &p);
            return Ok(if collision(&a, &b, &p) {
                AntiResult::Ok(reducer.runtime(), a, b)
            } else {
                AntiResult::Unknown
            });
        }
        if best.as_ref().is_none_or(|(old, _)| &norm < old) {
            best = Some((norm, differences.to_vec()));
        }
    }
    let best = best.map(|(_, row)| row);
    Ok(if reducer.check_time_out() {
        AntiResult::TimeOut(best)
    } else {
        AntiResult::NotFound(reducer.runtime(), best)
    })
}

/// Compatibility API; use `search` to receive structured input errors.
pub fn anti_hash(parameters: Parameters) -> AntiResult {
    anti_hash_with_bkz(parameters, BkzOptions::default())
}
pub fn anti_palindrome_hash(mut parameters: Parameters) -> AntiResult {
    parameters.palindrome = true;
    anti_hash(parameters)
}
/// Run L² followed by bounded projected-block BKZ.
pub fn anti_hash_with_bkz(parameters: Parameters, bkz: BkzOptions) -> AntiResult {
    search(
        parameters,
        SearchOptions {
            bkz,
            infinity_norm: false,
        },
    )
    .unwrap_or(AntiResult::Unknown)
}
/// Add exact local infinity-norm reduction after L²/BKZ.
pub fn anti_hash_with_infinity_norm(parameters: Parameters, bkz: BkzOptions) -> AntiResult {
    search(
        parameters,
        SearchOptions {
            bkz,
            infinity_norm: true,
        },
    )
    .unwrap_or(AntiResult::Unknown)
}

#[cfg(test)]
mod tests;
