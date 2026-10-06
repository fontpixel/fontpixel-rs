//! Synchronous JS callbacks behind `Send + Sync` Rust traits.
//!
//! Core and ops callback traits (`ResourceResolver`, `FileProvider`, codec
//! hooks, progress) require `Send + Sync`, which a `JsValue` is not. A
//! [`CallbackScope`] parks the JS object in a thread-local table for the
//! duration of one call and hands out a [`Callback`] token (an index, so
//! `Send + Sync`); the trait implementation calls through the token. WASM
//! runs on one thread here, so the table the token reads is the table the
//! scope filled. This needs no `unsafe` and no relaxed trait bounds.
//!
//! Errors: the first exception a callback throws is recorded (the trait
//! method reports a Rust error so the call unwinds normally); when the call
//! returns, [`CallbackScope::rethrow`] throws that original JS value (the
//! same object the callback threw), as the Python binding re-raises the
//! callback's exception.
use crate::errors::JsResult;
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

struct Slot {
    object: JsValue,
    error: Option<JsValue>,
}

thread_local! {
    static SLOTS: RefCell<Vec<Option<Slot>>> = const { RefCell::new(Vec::new()) };
}

/// A `Send + Sync` handle to a JS object parked by a [`CallbackScope`].
#[derive(Clone, Copy, Debug)]
pub struct Callback(usize);

impl Callback {
    /// The parked object (`None` once its scope ended).
    pub fn object(self) -> Option<JsValue> {
        SLOTS.with(|slots| {
            slots
                .borrow()
                .get(self.0)
                .and_then(|slot| slot.as_ref().map(|s| s.object.clone()))
        })
    }
    /// Record a JS exception thrown by the callback (the first one wins).
    pub fn record(self, error: JsValue) {
        SLOTS.with(|slots| {
            if let Some(Some(slot)) = slots.borrow_mut().get_mut(self.0) {
                slot.error.get_or_insert(error);
            }
        });
    }
}

/// Parks a JS object for the duration of one call (RAII).
pub struct CallbackScope {
    token: Callback,
}

impl CallbackScope {
    /// Park `object`.
    pub fn new(object: JsValue) -> Self {
        let index = SLOTS.with(|slots| {
            let mut slots = slots.borrow_mut();
            let slot = Some(Slot {
                object,
                error: None,
            });
            if let Some(free) = slots.iter().position(Option::is_none) {
                slots[free] = slot;
                free
            } else {
                slots.push(slot);
                slots.len() - 1
            }
        });
        Self {
            token: Callback(index),
        }
    }
    /// The token for trait implementations.
    pub fn token(&self) -> Callback {
        self.token
    }
    /// The first exception the callback threw, if any.
    pub fn take_error(&self) -> Option<JsValue> {
        SLOTS.with(|slots| {
            slots
                .borrow_mut()
                .get_mut(self.token.0)
                .and_then(|slot| slot.as_mut().and_then(|s| s.error.take()))
        })
    }
    /// `result`, unless the callback threw: then its own exception, which
    /// takes precedence even when the call recovered from the failure.
    pub fn rethrow<T>(&self, result: JsResult<T>) -> JsResult<T> {
        match self.take_error() {
            Some(error) => Err(error),
            None => result,
        }
    }
}

impl Drop for CallbackScope {
    fn drop(&mut self) {
        SLOTS.with(|slots| {
            if let Some(slot) = slots.borrow_mut().get_mut(self.token.0) {
                *slot = None;
            }
        });
    }
}
