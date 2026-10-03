//! Thin WASM-only transport and platform bridge; source coverage is reported separately.
use crate::model::Action;
use crate::{
    model::*,
    views::{Dashboard, Ui},
};
use gloo_net::http::Request as Fetch;
use leptos::prelude::*;
use serde_json::Value;
use std::{cell::RefCell, collections::BTreeMap};
use wasm_bindgen::JsCast;
use web_sys::{AbortController, HtmlDialogElement, HtmlElement, RequestCredentials};
thread_local! {
    static REQUESTS: RefCell<BTreeMap<Slot, AbortController>> = const { RefCell::new(BTreeMap::new()) };
    static PREVIOUS_FOCUS: RefCell<Option<HtmlElement>> = const { RefCell::new(None) };
}
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn mount() {
    let mut initial = State::default();
    let storage = web_sys::window().and_then(|w| w.local_storage().ok().flatten());
    if let Some(storage) = storage {
        initial.language = Language::from_preference(
            &storage
                .get_item("usage-lens:language")
                .ok()
                .flatten()
                .unwrap_or_default(),
        );
        initial.dark = storage
            .get_item("usage-lens:theme")
            .ok()
            .flatten()
            .as_deref()
            == Some("dark");
    }
    leptos::mount::mount_to_body(move || {
        let state = RwSignal::new(initial.clone());
        let revision = RwSignal::new(0u64);
        let modal_revision = RwSignal::new(0u64);
        let send = Callback::new(move |action: Action| {
            let (main_repaint, repaint) = action.repaint();
            if matches!(action, Action::Select(..) | Action::Confirm(..)) {
                PREVIOUS_FOCUS.with_borrow_mut(|previous| {
                    *previous = web_sys::window()
                        .and_then(|w| w.document())
                        .and_then(|d| d.active_element())
                        .and_then(|e| e.dyn_into::<HtmlElement>().ok());
                });
            }
            let requests = state.try_update(|s| s.dispatch(action)).unwrap_or_default();
            if main_repaint {
                revision.update(|v| *v += 1);
            }
            if repaint {
                modal_revision.update(|v| *v += 1);
            }
            synchronize(state);
            run(state, revision, modal_revision, requests);
        });
        let ui = Ui {
            state,
            revision,
            modal_revision,
            send,
        };
        Effect::new(move |_| {
            revision.get();
            modal_revision.get();
            synchronize(state);
        });
        send.run(Action::Refresh);
        view! { <Dashboard ui /> }
    });
}
fn run(
    state: RwSignal<State>,
    revision: RwSignal<u64>,
    modal_revision: RwSignal<u64>,
    requests: Vec<Request>,
) {
    for request in requests {
        let controller = AbortController::new().ok();
        REQUESTS.with_borrow_mut(|pending| {
            if let Some(old) = pending.remove(&request.slot) {
                old.abort();
            }
            if let Some(c) = &controller {
                pending.insert(request.slot, c.clone());
            }
        });
        wasm_bindgen_futures::spawn_local(async move {
            let response = async {
                let builder = if request.body.is_some() {
                    Fetch::post(&request.path)
                        .header("Content-Type", "application/json")
                        .header("X-Usage-Lens-Request", "local-ui")
                } else {
                    Fetch::get(&request.path)
                }
                .credentials(RequestCredentials::SameOrigin)
                .abort_signal(controller.as_ref().map(|c| c.signal()).as_ref());
                let response = if let Some(body) = &request.body {
                    builder
                        .body(body.to_string())
                        .map_err(|_| "request_failed".to_owned())?
                        .send()
                        .await
                } else {
                    builder.send().await
                }
                .map_err(|_| "request_failed".to_owned())?;
                let ok = response.ok();
                let data = response
                    .json::<Value>()
                    .await
                    .map_err(|_| "invalid_response".to_owned())?;
                response_result(ok, data)
            }
            .await;
            if !state.with_untracked(|s| s.accepts(&request)) {
                return;
            }
            let next = state
                .try_update(|s| s.complete(&request, response))
                .unwrap_or_default();
            if !matches!(request.slot, Slot::Detail | Slot::TraceDetail) {
                revision.update(|v| *v += 1);
            }
            modal_revision.update(|v| *v += 1);
            synchronize(state);
            run(state, revision, modal_revision, next);
        });
    }
}
fn synchronize(state: RwSignal<State>) {
    let s = state.get_untracked();
    REQUESTS.with_borrow_mut(|pending| {
        pending.retain(|slot, controller| {
            let live = s.loading(*slot);
            if !live {
                controller.abort();
            }
            live
        })
    });
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    if let Some(root) = document.document_element() {
        let _ = root.set_attribute("lang", s.language.html_lang());
        let _ = root.set_attribute("data-theme", if s.dark { "dark" } else { "light" });
    }
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item("usage-lens:language", s.language.code());
        let _ = storage.set_item("usage-lens:theme", if s.dark { "dark" } else { "light" });
    }
    leptos::prelude::queue_microtask(move || {
        let dialog = document
            .query_selector("dialog[data-modal]")
            .ok()
            .flatten()
            .and_then(|e| e.dyn_into::<HtmlDialogElement>().ok());
        if let Some(dialog) = dialog {
            if !dialog.open() {
                PREVIOUS_FOCUS.with_borrow_mut(|previous| {
                    if previous.is_none() {
                        *previous = document
                            .active_element()
                            .and_then(|e| e.dyn_into::<HtmlElement>().ok());
                    }
                });
                let _ = dialog.show_modal();
            }
        } else {
            PREVIOUS_FOCUS.with_borrow_mut(|previous| {
                if let Some(element) = previous.take().filter(|e| e.is_connected()) {
                    let _ = element.focus();
                }
            });
        }
    });
}
