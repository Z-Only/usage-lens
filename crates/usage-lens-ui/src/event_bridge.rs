//! DOM extraction only. Native SSR does not execute these wrappers; coverage must count them uncovered.
use crate::{
    model::{Action, InputAction},
    views::Ui,
};
use leptos::prelude::{event_target_checked, event_target_value};
use wasm_bindgen::JsCast;
pub fn value<E: JsCast>(ui: Ui, action: InputAction) -> impl Fn(E) {
    move |event| ui.send(action.action(event_target_value(&event)))
}
pub fn checked(ui: Ui, key: &'static str) -> impl Fn(web_sys::Event) {
    move |event| {
        ui.send(Action::Setting(
            key,
            serde_json::json!(event_target_checked(&event)),
        ))
    }
}
pub fn prevent<E: JsCast>(ui: Ui, action: Action) -> impl Fn(E) {
    move |event: E| {
        event.unchecked_ref::<web_sys::Event>().prevent_default();
        ui.send(action.clone());
    }
}

pub fn cancel(ui: Ui) -> impl Fn(web_sys::Event) {
    prevent(ui, Action::Close)
}
