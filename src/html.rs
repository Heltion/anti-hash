use std::str::FromStr;

use bigdecimal::{num_bigint::BigInt, BigDecimal, FromPrimitive, One};
use wasm_bindgen::{closure::Closure, prelude::wasm_bindgen, JsCast, JsValue};
use web_sys::{
    Document, Element, HtmlButtonElement, HtmlDivElement, HtmlElement, HtmlInputElement, Node,
};

use crate::anti_hash::{anti_hash, Parameters};
const N: usize = 4;
const DEFAULT_MODULO: [&str; 4] = [
    "998244353",
    "1000000007",
    "1000000009",
    "1000000000000000003",
];
const DEFAULT_BASE: [&str; 4] = ["233", "27", "257", "114514"];
const DEFAULT_LENGTH: &str = "30";
const DEFAULT_PRECISION: &str = "10";
#[allow(unused)]
fn console_log(s: String) {
    web_sys::console::log_1(&JsValue::from_str(&s));
}

fn document() -> Document {
    web_sys::window().unwrap().document().unwrap()
}

fn element(local_name: &str) -> Element {
    document().create_element(local_name).unwrap()
}

fn input() -> HtmlInputElement {
    element("input").dyn_into().unwrap()
}

fn button() -> HtmlButtonElement {
    element("button").dyn_into().unwrap()
}

fn div<const T: usize>(nodes: [Node; T]) -> HtmlDivElement {
    let div = element("div").dyn_into::<HtmlDivElement>().unwrap();
    for node in nodes {
        div.append_child(&node).unwrap();
    }
    div
}

trait WithDefaultValue
where
    Self: Into<HtmlInputElement>,
{
    fn with_default_value(self, value: &str) -> HtmlInputElement {
        let input: HtmlInputElement = self.into();
        input.set_default_value(value);
        input
    }
}
impl WithDefaultValue for HtmlInputElement {}

trait WithType
where
    Self: Into<HtmlInputElement>,
{
    fn with_type(self, value: &str) -> HtmlInputElement {
        let input: HtmlInputElement = self.into();
        input.set_type(value);
        input
    }
}
impl WithType for HtmlInputElement {}

trait WithId
where
    Self: Into<Element>,
{
    fn with_id(self, value: &str) -> Element {
        let element: Element = self.into();
        element.set_id(value);
        element
    }
}
impl WithId for Element {}
impl WithId for HtmlButtonElement {}
impl WithId for HtmlDivElement {}
impl WithId for HtmlInputElement {}

trait WithAtrribute
where
    Self: Into<Element>,
{
    fn with_atrribute(self, name: &str, value: &str) -> Element {
        let element: Element = self.into();
        element.set_attribute(name, value).unwrap();
        element
    }
}
impl WithAtrribute for Element {}
impl WithAtrribute for HtmlInputElement {}

trait WithTextContent
where
    Self: Into<Node>,
{
    fn with_text_content(self, value: &str) -> Node {
        let node: Node = self.into();
        node.set_text_content(Some(value));
        node
    }
}
impl WithTextContent for Element {}

fn update_input() {
    let number: usize = match get_value("number").parse() {
        Ok(number) => {
            if number == 0 || number > N {
                return;
            }
            number
        }
        Err(_) => {
            return;
        }
    };
    for i in 0..N {
        let input = document()
            .get_element_by_id(&format!("input_{}", i))
            .unwrap()
            .dyn_into::<HtmlElement>()
            .unwrap();
        input
            .style()
            .set_property("display", if i < number { "block" } else { "none" })
            .unwrap();
    }
}

fn get_value(id: &str) -> String {
    document()
        .get_element_by_id(id)
        .unwrap()
        .dyn_into::<HtmlInputElement>()
        .unwrap()
        .value()
}

fn get_value_parsed<T: FromStr>(id: &str, td: &str) -> Result<T, String> {
    match get_value(id).parse() {
        Ok(val) => Ok(val),
        Err(_) => Err(format!("[{}] should be {}.", id, td)),
    }
}

fn get_inputs() -> Result<Parameters, String> {
    let number: usize = get_value_parsed("number", "an unsigned 64-bit integer")?;
    if number == 0 || number > 8 {
        return Err("[number] should be between 1 and 8.".to_string());
    }
    let length = get_value_parsed("length", "an unsigned 64-bit integer")?;
    if length == 0 {
        return Err("[length] should be postive.".to_string());
    }
    let mut modulo = Vec::new();
    for i in 0..number {
        modulo.push(get_value_parsed(&format!("modulo_{}", i), "an integer")?);
    }
    let mut base = Vec::new();
    for i in 0..number {
        base.push(get_value_parsed(&format!("base_{}", i), "an integer")?);
    }
    for i in 0..number {
        if base[i] < BigInt::ZERO {
            return Err(format!("[base_{}] should be non-negative.", i));
        }
        if base[i] >= modulo[i] {
            return Err(format!(
                "[base_{}] should be strictly less than [modulo_{}].",
                i, i
            ));
        }
    }
    let lambda = get_value_parsed("lambda", "an intger")?;
    if lambda <= BigInt::ZERO {
        return Err("[lambda] should be positive".to_string());
    }
    let delta = get_value_parsed("delta", "a number")?;
    if delta >= BigDecimal::one() {
        return Err("[delta] should be strictly less than 1.".to_string());
    }
    let eta = get_value_parsed("eta", "a number")?;
    if eta <= BigDecimal::from_f64(0.5).unwrap() {
        return Err("[eta] should be strictly greater than 0.5".to_string());
    }
    let precision = get_value_parsed("precision", "an unsigned 64-bit integer")?;
    if precision == 0 || precision > 100 {
        return Err("[precision] should be between 1 and 100".to_string());
    }
    let timeout = get_value_parsed("timeout", "a number")?;
    if timeout <= 0. {
        return Err("[timeout] should be positive".to_string());
    }
    Ok(Parameters {
        length,
        modulo,
        base,
        lambda,
        delta,
        eta,
        precision,
        timeout,
    })
}
fn run_anti_hash() {
    let generate = document()
        .get_element_by_id("generate")
        .unwrap()
        .dyn_into::<HtmlButtonElement>()
        .unwrap();
    generate.set_disabled(true);
    let outputs = document().get_element_by_id("outputs").unwrap();
    let input = match get_inputs() {
        Ok(parameters) => parameters,
        Err(err) => {
            outputs.set_text_content(Some(&err));
            generate.set_disabled(false);
            return;
        }
    };
    let result = anti_hash(input);
    outputs.set_text_content(None);
    match result {
        crate::anti_hash::AntiResult::NotFound(time, best) => {
            outputs
                .append_child(
                    &element("div").with_text_content(&format!("time consumed: {}s", time)),
                )
                .unwrap();
            outputs
                .append_child(&element("div").with_text_content("Not found."))
                .unwrap();
            if let Some(best) = best {
                outputs
                    .append_child(&element("div").with_text_content(&format!(
                        "The hashes of the following array are zeros {:?}",
                        best
                    )))
                    .unwrap();
            }
        }
        crate::anti_hash::AntiResult::TimeOut(best) => {
            outputs.set_text_content(Some("Timeout."));
            if let Some(best) = best {
                outputs
                    .append_child(&element("div").with_text_content(&format!(
                        "The hashes of the following array are zeros {:?}",
                        best
                    )))
                    .unwrap();
            }
        }
        crate::anti_hash::AntiResult::Ok(time, a, b) => {
            outputs
                .append_child(
                    &element("div").with_text_content(&format!("time consumend: {}s", time)),
                )
                .unwrap();
            outputs
                .append_child(&element("div").with_text_content(&a))
                .unwrap();
            outputs
                .append_child(&element("div").with_text_content(&b))
                .unwrap();
        }
        crate::anti_hash::AntiResult::Unknown => {
            outputs.set_text_content(Some("There exist unknown bugs."))
        }
    }
    generate.set_disabled(false);
}
fn update_output() {
    let outputs = document().get_element_by_id("outputs").unwrap();
    outputs.set_text_content(Some("reducing..."));
}

#[wasm_bindgen(start)]
fn main() {
    let app = document().get_element_by_id("app").unwrap();
    app.append_child(&element("h1").with_text_content("Anti-hash Test Generator"))
        .unwrap();
    let a = element("a").with_atrribute(
        "href",
        &format!(
            "https://github.com/{}/{}",
            env!("CARGO_PKG_AUTHORS"),
            env!("CARGO_PKG_NAME")
        ),
    );
    a.append_child(&element("img").with_atrribute(
        "src",
        &format!(
            "https://img.shields.io/github/stars/{}/{}",
            env!("CARGO_PKG_AUTHORS"),
            env!("CARGO_PKG_NAME")
        ),
    ))
    .unwrap();
    app.append_child(&a).unwrap();
    app.append_child(&div([
        element("span").with_text_content("length: "),
        input()
            .with_type("number")
            .with_default_value(DEFAULT_LENGTH)
            .with_id("length")
            .into(),
    ]))
    .unwrap();
    app.append_child(&div([
        element("span").with_text_content("number of modulo-base pairs: "),
        input()
            .with_type("number")
            .with_default_value("1")
            .with_id("number")
            .with_atrribute("min", "1")
            .with_atrribute("max", &N.to_string())
            .into(),
    ]))
    .unwrap();
    for i in 0..N {
        app.append_child(
            &div([
                element("span").with_text_content(&format!("modulo_{}: ", i)),
                input()
                    .with_type("number")
                    .with_default_value(DEFAULT_MODULO[i])
                    .with_id(&format!("modulo_{}", i))
                    .into(),
                element("span").with_text_content(&format!(" base_{}: ", i)),
                input()
                    .with_type("number")
                    .with_default_value(DEFAULT_BASE[i])
                    .with_id(&format!("base_{}", i))
                    .into(),
            ])
            .with_id(&format!("input_{}", i))
            .into(),
        )
        .unwrap();
    }
    app.append_child(&element("br").into()).unwrap();
    app.append_child(&element("br").into()).unwrap();
    app.append_child(&div([
        element("span").with_text_content("lambda = "),
        input()
            .with_type("number")
            .with_default_value("100000")
            .with_id("lambda")
            .into(),
    ]))
    .unwrap();
    app.append_child(&div([
        element("span").with_text_content("delta = "),
        input()
            .with_type("number")
            .with_default_value("0.99")
            .with_id("delta")
            .into(),
    ]))
    .unwrap();
    app.append_child(&div([
        element("span").with_text_content("eta = "),
        input()
            .with_type("number")
            .with_default_value("0.51")
            .with_id("eta")
            .into(),
    ]))
    .unwrap();
    app.append_child(&div([
        element("span").with_text_content("precision: "),
        input()
            .with_type("number")
            .with_default_value(DEFAULT_PRECISION)
            .with_id("precision")
            .into(),
    ]))
    .unwrap();
    app.append_child(&div([
        element("span").with_text_content("timeout in seconds: "),
        input().with_default_value("60").with_id("timeout").into(),
    ]))
    .unwrap();
    app.append_child(&button().with_id("generate").with_text_content("generate"))
        .unwrap();
    app.append_child(&element("div").with_id("outputs").into())
        .unwrap();

    let number = document().get_element_by_id("number").unwrap();
    let closure = Closure::<dyn Fn()>::new(update_input);
    number
        .add_event_listener_with_callback("input", closure.as_ref().unchecked_ref())
        .unwrap();
    closure.forget();
    update_input();
    let generate = document().get_element_by_id("generate").unwrap();
    let closure = Closure::<dyn Fn()>::new(update_output);
    generate
        .add_event_listener_with_callback("mousedown", closure.as_ref().unchecked_ref())
        .unwrap();
    closure.forget();
    let closure = Closure::<dyn Fn()>::new(run_anti_hash);
    generate
        .add_event_listener_with_callback("mouseup", closure.as_ref().unchecked_ref())
        .unwrap();
    closure.forget();
}
