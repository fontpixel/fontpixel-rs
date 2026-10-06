//! Synchronous JS callbacks behind Rust callback traits.
//!
//! On `target_family = "wasm"` the core/ops callback traits
//! (`ResourceResolver`, `FileProvider`, `FormatDetector`/`Reader`/`Writer`)
//! carry no `Send + Sync` bound, but on native hosts (where this crate also
//! compiles for the workspace clippy/test runs) they do, and a `JsValue` is
//! neither. A [`CallbackScope`] therefore parks the JS object in a
//! thread-local table for the duration of one call and hands out a
//! [`Callback`] token (an index, so `Send + Sync` everywhere); the trait
//! implementation calls through the token. WASM runs on one thread here, so
//! the table the token reads is the table the scope filled. No `unsafe`.
//!
//! Progress observers (`fontpixel_ops::merge_with_progress`,
//! `convert_with_progress`) take a plain `FnMut` and need no token:
//! [`progress_observer`] adapts a JS function.
//!
//! Errors: the first exception a callback throws is recorded (the trait
//! method reports a Rust error so the call unwinds normally); when the call
//! returns, [`CallbackScope::rethrow`] throws that original JS value (the
//! same object the callback threw), as the Python binding re-raises the
//! callback's exception.
use crate::errors::JsResult;
use std::cell::RefCell;
use wasm_bindgen::JsCast;
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

/// A JS progress callback for the ops `*_with_progress` operations.
///
/// The callback receives the plain-data `Progress` (`{step, done, total,
/// item, glyphs, bytes}`). Only a return of exactly `false` cancels (the
/// operation then fails with `CancelledError`, nothing written or
/// returned); any other return value continues. A callback that throws also
/// cancels, and its exception is what [`ProgressCallback::finish`] throws,
/// mirroring the Python `progress=` keyword.
pub struct ProgressCallback {
    function: js_sys::Function,
    error: RefCell<Option<JsValue>>,
}

impl ProgressCallback {
    /// Wrap `callback` (a function; `undefined`/`null` gives `None`).
    pub fn new(callback: &JsValue) -> JsResult<Option<Self>> {
        if callback.is_undefined() || callback.is_null() {
            return Ok(None);
        }
        let function = callback
            .clone()
            .dyn_into::<js_sys::Function>()
            .map_err(|_| crate::errors::invalid("progress must be a function"))?;
        Ok(Some(Self {
            function,
            error: RefCell::new(None),
        }))
    }

    /// The observer to pass to `merge_with_progress` / `convert_with_progress`.
    pub fn observer(
        &self,
    ) -> impl FnMut(&fontpixel_ops::Progress) -> std::ops::ControlFlow<()> + '_ {
        use std::ops::ControlFlow;
        move |progress| {
            let value = match crate::conv::to_js(progress) {
                Ok(value) => value,
                Err(error) => {
                    self.error.borrow_mut().get_or_insert(error);
                    return ControlFlow::Break(());
                }
            };
            match self.function.call1(&JsValue::UNDEFINED, &value) {
                Ok(result) if result.as_bool() == Some(false) => ControlFlow::Break(()),
                Ok(_) => ControlFlow::Continue(()),
                Err(error) => {
                    self.error.borrow_mut().get_or_insert(error);
                    ControlFlow::Break(())
                }
            }
        }
    }

    /// `result`, unless the callback threw: then its own exception.
    pub fn finish<T>(&self, result: JsResult<T>) -> JsResult<T> {
        match self.error.borrow_mut().take() {
            Some(error) => Err(error),
            None => result,
        }
    }
}
