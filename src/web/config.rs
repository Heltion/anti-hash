//! DOM-independent WebAssembly entry points shared by the page and workers.
use crate::{Alphabet, AntiResult, BkzOptions, Parameters, SearchOptions, search};
use js_sys::{Array, Reflect};
use malachite::base::num::conversion::traits::FromSciString;
use malachite::{Float, Rational};
use std::str::FromStr;
use wasm_bindgen::prelude::*;

fn field(config: &JsValue, name: &str) -> Result<String, JsValue> {
    Reflect::get(config, &JsValue::from_str(name))?
        .as_string()
        .ok_or_else(|| JsValue::from_str(&format!("Missing field: {name}")))
}
fn parse<T: FromStr>(config: &JsValue, name: &str) -> Result<T, JsValue> {
    field(config, name)?
        .trim()
        .parse()
        .map_err(|_| JsValue::from_str(&format!("[{name}] has an invalid value.")))
}
fn parameters(config: &JsValue) -> Result<(Parameters, SearchOptions), JsValue> {
    let count: usize = parse(config, "number")?;
    if !(1..=100).contains(&count) {
        return Err("Number of pairs must be between 1 and 100.".into());
    }
    let precision: u64 = parse(config, "precision")?;
    if !(1..=100).contains(&precision) {
        return Err("Precision must be between 1 and 100.".into());
    }
    let decimal = |name| {
        let value = field(config, name)?;
        let rational = Rational::from_sci_string(value.trim()).ok_or_else(|| {
            JsValue::from_str(&format!("[{name}] must be a finite decimal number."))
        })?;
        Ok::<_, JsValue>(
            Float::from_rational_prec(
                rational,
                (precision as f64 * std::f64::consts::LOG2_10).ceil() as u64 + 10,
            )
            .0,
        )
    };
    let p = Parameters {
        length: parse(config, "length")?,
        modulo: (0..count)
            .map(|i| parse(config, &format!("modulo_{i}")))
            .collect::<Result<_, _>>()?,
        base: (0..count)
            .map(|i| parse(config, &format!("base_{i}")))
            .collect::<Result<_, _>>()?,
        lambda: parse(config, "lambda")?,
        delta: decimal("delta")?,
        eta: decimal("eta")?,
        precision,
        sigma: parse(config, "sigma")?,
        timeout: parse(config, "timeout")?,
        palindrome: parse(config, "palindrome")?,
    };
    p.validate()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let options = SearchOptions {
        bkz: BkzOptions {
            block_size: parse(config, "block_size")?,
            max_tours: parse(config, "max_tours")?,
        },
        infinity_norm: parse(config, "infinity_norm")?,
        alphabet: match field(config, "alphabet")?.as_str() {
            "letters" => Alphabet::Letters,
            "digits" => Alphabet::Digits,
            "both" => Alphabet::Both,
            _ => return Err("Unknown alphabet.".into()),
        },
    };
    if !(2..=40).contains(&options.bkz.block_size) || !(1..=100).contains(&options.bkz.max_tours) {
        return Err("BKZ block size must be 2–40 and tours 1–100.".into());
    }
    if p.sigma > options.alphabet.symbols().len() {
        return Err("Size exceeds the selected alphabet.".into());
    }
    Ok((p, options))
}

/// Validate cheaply before launching any workers or starting the shared deadline.
pub(super) fn validate_config(config: &JsValue) -> Result<(), JsValue> {
    parameters(config).map(|_| ())
}

/// One complete fixed-length search. Strings retain arbitrary-precision integer inputs.
pub(super) fn solve(config: &JsValue) -> Result<Array, JsValue> {
    let (parameters, options) = parameters(config)?;
    let result = search(parameters, options).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let output = Array::new();
    match result {
        AntiResult::Ok(time, a, b) => {
            output.push(&"ok".into());
            output.push(&time.into());
            output.push(&a.into());
            output.push(&b.into());
        }
        AntiResult::NotFound(time, _) => {
            output.push(&"not_found".into());
            output.push(&time.into());
        }
        AntiResult::TimeOut(_) => {
            output.push(&"timeout".into());
        }
        AntiResult::Unknown => {
            return Err("Collision verification failed. Try increasing precision.".into());
        }
    }
    Ok(output)
}
