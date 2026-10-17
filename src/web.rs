//! Browser adapter; the native library does not depend on a DOM or JavaScript runtime.
use crate::{AntiResult, BkzOptions, Parameters, SearchOptions, search};
use malachite::base::num::conversion::traits::FromSciString;
use malachite::{Float, Rational};
use std::{cell::RefCell, rc::Rc, str::FromStr};
use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};
use web_sys::{Document, Element, Event, EventTarget, HtmlButtonElement, HtmlInputElement};

const MODULI: [&str; 4] = [
    "998244353",
    "1000000007",
    "1000000000000000003",
    "18446744073709551616",
];
const BASES: [&str; 4] = ["233", "27", "257", "114514"];
const DIRECTIONS: [&str; 2] = ["h ≡ ∑ si q^i (mod p)", "h ≡ ∑ si q^(n-1-i) (mod p)"];

/// Remove listeners before releasing their Rust closures.
struct Listener {
    target: EventTarget,
    event: &'static str,
    callback: Closure<dyn FnMut(Event)>,
}
impl Listener {
    fn new(
        target: EventTarget,
        event: &'static str,
        callback: impl FnMut(Event) + 'static,
    ) -> Result<Self, JsValue> {
        let callback = Closure::new(callback);
        target.add_event_listener_with_callback(event, callback.as_ref().unchecked_ref())?;
        Ok(Self {
            target,
            event,
            callback,
        })
    }
}
impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self.target.remove_event_listener_with_callback(
            self.event,
            self.callback.as_ref().unchecked_ref(),
        );
    }
}
thread_local! { static LISTENERS: RefCell<Vec<Listener>> = const { RefCell::new(Vec::new()) }; }

struct App {
    document: Document,
    rows: RefCell<Vec<Element>>,
    generate: HtmlButtonElement,
    log: Element,
    outputs: Element,
}
impl App {
    fn element(&self, id: &str) -> Element {
        self.document
            .get_element_by_id(id)
            .expect("application markup")
    }
    fn input(&self, id: &str) -> HtmlInputElement {
        self.element(id).dyn_into().expect("input element")
    }
    fn parse<T: FromStr>(&self, id: &str) -> Result<T, String> {
        self.input(id)
            .value()
            .trim()
            .parse()
            .map_err(|_| format!("[{id}] has an invalid value."))
    }
    fn count(&self) -> Result<usize, String> {
        let n = self.parse("number")?;
        if (1..=100).contains(&n) {
            Ok(n)
        } else {
            Err("Number of pairs must be between 1 and 100.".into())
        }
    }
    fn update_rows(&self) -> Result<(), JsValue> {
        let Ok(count) = self.count() else {
            return Ok(());
        };
        let mut rows = self.rows.borrow_mut();
        while rows.len() < count {
            let i = rows.len();
            let row = self.document.create_element("div")?;
            row.set_id(&format!("input_{i}"));
            row.set_class_name("hash-pair");
            for (kind, value) in [
                ("modulo", MODULI.get(i).copied().unwrap_or("1")),
                ("base", BASES.get(i).copied().unwrap_or("0")),
            ] {
                let label = self.document.create_element("label")?;
                label.set_text_content(Some(&format!("{kind}_{i}: ")));
                let input: HtmlInputElement = self.document.create_element("input")?.dyn_into()?;
                input.set_id(&format!("{kind}_{i}"));
                input.set_attribute("inputmode", "numeric")?;
                input.set_value(value);
                label.append_child(&input)?;
                row.append_child(&label)?;
            }
            self.element("hashes").append_child(&row)?;
            rows.push(row);
        }
        for (i, row) in rows.iter().enumerate() {
            row.toggle_attribute_with_force("hidden", i >= count)?;
        }
        Ok(())
    }
    fn parameters(&self) -> Result<(Parameters, SearchOptions), String> {
        let count = self.count()?;
        let precision: u64 = self.parse("precision")?;
        if !(1..=100).contains(&precision) {
            return Err("Precision must be between 1 and 100 decimal digits.".into());
        }
        let decimal = |id: &str| {
            let value = self.input(id).value();
            // Parse at requested precision without first rounding through binary64.
            let rational = Rational::from_sci_string(value.trim())
                .ok_or_else(|| format!("[{id}] must be a finite decimal number."))?;
            Ok::<_, String>(
                Float::from_rational_prec(
                    rational,
                    (precision as f64 * std::f64::consts::LOG2_10).ceil() as u64 + 10,
                )
                .0,
            )
        };
        let p = Parameters {
            length: self.parse("length")?,
            modulo: (0..count)
                .map(|i| self.parse(&format!("modulo_{i}")))
                .collect::<Result<_, _>>()?,
            base: (0..count)
                .map(|i| self.parse(&format!("base_{i}")))
                .collect::<Result<_, _>>()?,
            lambda: self.parse("lambda")?,
            delta: decimal("delta")?,
            eta: decimal("eta")?,
            precision,
            sigma: self.parse("sigma")?,
            timeout: self.parse("timeout")?,
            palindrome: self.input("palindrome").checked(),
        };
        p.validate().map_err(|e| e.to_string())?;
        let bkz = BkzOptions {
            block_size: self.parse("block_size")?,
            max_tours: self.parse("max_tours")?,
        };
        if !(2..=40).contains(&bkz.block_size) || !(1..=100).contains(&bkz.max_tours) {
            return Err("BKZ block size must be 2–40 and tours 1–100.".into());
        }
        Ok((
            p,
            SearchOptions {
                bkz,
                infinity_norm: self.input("infinity_norm").checked(),
            },
        ))
    }
    fn show(&self, result: Result<AntiResult, crate::InputError>) {
        let message = match result {
            Ok(AntiResult::Ok(time, a, b)) => {
                self.element("string_a").set_text_content(Some(&a));
                self.element("string_b").set_text_content(Some(&b));
                self.element("reverse_description")
                    .set_text_content(Some(DIRECTIONS[0]));
                self.outputs
                    .remove_attribute("hidden")
                    .expect("output element");
                format!("Collision found. Time consumed: {time:.6}s")
            }
            Ok(AntiResult::NotFound(time, best)) => {
                format!("Not found. Time consumed: {time:.6}s{}", best_message(best))
            }
            Ok(AntiResult::TimeOut(best)) => format!("Timeout.{}", best_message(best)),
            Ok(AntiResult::Unknown) => {
                "Collision verification failed. Try increasing precision.".into()
            }
            Err(error) => error.to_string(),
        };
        self.log.set_text_content(Some(&message));
        self.generate.set_disabled(false);
    }
    fn submit(self: &Rc<Self>) {
        if self.generate.disabled() {
            return;
        }
        self.outputs
            .set_attribute("hidden", "")
            .expect("output element");
        let (parameters, options) = match self.parameters() {
            Ok(input) => input,
            Err(error) => {
                self.log.set_text_content(Some(&error));
                return;
            }
        };
        self.generate.set_disabled(true);
        self.log.set_text_content(Some("Reducing…"));
        let app = Rc::clone(self);
        // Yield to the event loop so keyboard and pointer submission use the same path.
        let callback = Closure::once_into_js(move || app.show(search(parameters, options)));
        if let Err(error) = web_sys::window()
            .expect("window")
            .set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), 0)
        {
            self.log
                .set_text_content(Some(&format!("Unable to start search: {error:?}")));
            self.generate.set_disabled(false);
        }
    }
    fn reverse(&self) {
        let description = self.element("reverse_description");
        let reversed = description.text_content().as_deref() == Some(DIRECTIONS[1]);
        description.set_text_content(Some(DIRECTIONS[usize::from(!reversed)]));
        for id in ["string_a", "string_b"] {
            let output = self.element(id);
            output.set_text_content(Some(
                &output
                    .text_content()
                    .unwrap_or_default()
                    .chars()
                    .rev()
                    .collect::<String>(),
            ));
        }
    }
}
fn best_message(best: Option<Vec<malachite::Integer>>) -> String {
    best.map_or_else(String::new, |row| {
        format!("\nBest zero-residue differences: {row:?}")
    })
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let document = web_sys::window()
        .ok_or("window unavailable")?
        .document()
        .ok_or("document unavailable")?;
    let element = |id| {
        document
            .get_element_by_id(id)
            .ok_or_else(|| JsValue::from_str(&format!("Missing #{id}")))
    };
    let app = Rc::new(App {
        generate: element("generate")?.dyn_into()?,
        log: element("log")?,
        outputs: element("outputs")?,
        document,
        rows: RefCell::new(Vec::new()),
    });
    app.update_rows()?;
    let submit_app = Rc::clone(&app);
    let rows_app = Rc::clone(&app);
    let reverse_app = Rc::clone(&app);
    let listeners = vec![
        Listener::new(app.element("search").into(), "submit", move |event| {
            event.prevent_default();
            submit_app.submit();
        })?,
        Listener::new(app.input("number").into(), "input", move |_| {
            if let Err(error) = rows_app.update_rows() {
                rows_app
                    .log
                    .set_text_content(Some(&format!("Unable to update inputs: {error:?}")));
            }
        })?,
        Listener::new(app.element("reverse").into(), "click", move |_| {
            reverse_app.reverse()
        })?,
    ];
    LISTENERS.with(|stored| *stored.borrow_mut() = listeners);
    app.log.set_text_content(None);
    app.generate.set_disabled(false);
    Ok(())
}
