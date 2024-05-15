use num::BigInt;
use wasm_bindgen::{closure::Closure, prelude::wasm_bindgen, JsCast, JsValue};
use web_sys::{
    Document, Element, HtmlButtonElement, HtmlDivElement, HtmlInputElement, HtmlLabelElement, Node,
};
const N: usize = 8;

#[allow(unused)]
fn console_log<T: ToString>(t: &T) {
    web_sys::console::log_1(&JsValue::from_str(&t.to_string()));
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

fn label() -> HtmlLabelElement {
    element("label").dyn_into().unwrap()
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

trait WithFor
where
    Self: Into<HtmlLabelElement>,
{
    fn with_for(self, value: &str) -> HtmlLabelElement {
        let label: HtmlLabelElement = self.into();
        label.set_html_for(value);
        label
    }
}
impl WithFor for HtmlLabelElement {}

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
impl WithTextContent for HtmlLabelElement {}

fn get_value(id: &str) -> String {
    document()
        .get_element_by_id(id)
        .unwrap()
        .dyn_into::<HtmlInputElement>()
        .unwrap()
        .value()
}

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
            .unwrap();
        input
            .set_attribute(
                "style",
                if i < number {
                    "display: block"
                } else {
                    "display: none"
                },
            )
            .unwrap();
    }
}

fn get_big_int(id: &str) -> Result<BigInt, String> {
    let value = get_value(id);
    match value.parse() {
        Ok(bi) => Ok(bi),
        Err(_) => Err(format!("Error: [{}] should be an integer", id)),
    }
}
fn update_output() {
    let outputs = document().get_element_by_id("outputs").unwrap();
    outputs.set_text_content(None);
    let number: usize = match get_value("number").parse() {
        Ok(number) => {
            if number == 0 || number > N {
                outputs.set_text_content(Some(
                    "Error: [number of modulo-base pairs] should be an integer bewteen 1 and 8.",
                ));
                return;
            }
            number
        }
        Err(_) => {
            outputs.set_text_content(Some(
                "Error: [number of modulo-base pairs] should be an integer bewteen 1 and 8.",
            ));
            return;
        }
    };
    let mut modulo_base = Vec::new();
    for i in 0..number {
        let modulo = match get_big_int(&format!("modulo_{}", i)) {
            Ok(bi) => bi,
            Err(err) => {
                outputs.set_text_content(Some(&err));
                return;
            }
        };
        let base = match get_big_int(&format!("base_{}", i)) {
            Ok(bi) => bi,
            Err(err) => {
                outputs.set_text_content(Some(&err));
                return;
            }
        };
        if base < BigInt::from(0) {
            outputs.set_text_content(Some(&format!(
                "Error: [base_{}] should be non-negative.",
                i
            )));
            return;
        }
        if base >= modulo {
            outputs.set_text_content(Some(&format!(
                "Error: [base_{}] should be strictly less than [modulo_{}].",
                i, i
            )));
            return;
        }
        let binary: HtmlInputElement = document()
            .get_element_by_id("binary")
            .unwrap()
            .dyn_into()
            .unwrap();
        let binary = binary.checked();
        modulo_base.push((modulo, base));
        outputs.set_text_content(Some(&format!("{} {:?}", binary, modulo_base)));
    }
}

#[wasm_bindgen(start)]
fn main() {
    let app = document().get_element_by_id("app").unwrap();
    app.append_child(&element("h1").with_text_content("Anti-hash Test Generator"))
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
    app.append_child(&div([
        label()
            .with_for("binary")
            .with_text_content("binary string"),
        input().with_type("checkbox").with_id("binary").into(),
        element("span").with_text_content(" (The output consists of only 'a' and 'b' if checked.)"),
    ]))
    .unwrap();
    for i in 0..N {
        app.append_child(
            &div([
                element("span").with_text_content(&format!("modulo_{}: ", i)),
                input()
                    .with_type("number")
                    .with_default_value("998244353")
                    .with_id(&format!("modulo_{}", i))
                    .into(),
                element("span").with_text_content(&format!(" base_{}: ", i)),
                input()
                    .with_type("number")
                    .with_default_value("233")
                    .with_id(&format!("base_{}", i))
                    .into(),
            ])
            .with_id(&format!("input_{}", i))
            .into(),
        )
        .unwrap();
    }
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
        .add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())
        .unwrap();
    closure.forget();
}
