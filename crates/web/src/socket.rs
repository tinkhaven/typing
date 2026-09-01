//! The browser end of the WebSocket.
//!
//! Deliberately a set of free functions over a thread-local rather than a value
//! that gets stored in a signal. A `WebSocket` and its event closures are not
//! `Send`, Leptos signals want values that are, and wasm is single-threaded
//! anyway — so keeping the connection out of the reactive graph avoids a pile of
//! wrapper types for no benefit.
//!
//! Under `ssr` every function here is a no-op, so components can call them
//! unconditionally and server rendering simply does nothing.

use crate::protocol::{ClientMessage, ServerMessage};

/// How the connection currently stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Never attempted, or deliberately closed.
    Idle,
    /// Opening.
    Connecting,
    /// Open and usable.
    Open,
    /// Closed or failed. Practice continues; results cannot be published.
    Closed,
}

#[cfg(feature = "hydrate")]
mod browser {
    use std::cell::RefCell;

    use wasm_bindgen::{prelude::Closure, JsCast};
    use web_sys::{ErrorEvent, MessageEvent, WebSocket};

    use super::{ClientMessage, ServerMessage, Status};

    /// The live connection plus the closures that must outlive this function.
    struct Connection {
        socket: WebSocket,
        // Dropping a Closure detaches it from the event target, so these are
        // held for as long as the socket is.
        _on_open: Closure<dyn FnMut()>,
        _on_message: Closure<dyn FnMut(MessageEvent)>,
        _on_close: Closure<dyn FnMut()>,
        _on_error: Closure<dyn FnMut(ErrorEvent)>,
    }

    thread_local! {
        static CONNECTION: RefCell<Option<Connection>> = const { RefCell::new(None) };
    }

    /// Builds `ws(s)://<host>/api/ws` from the page's own location.
    fn endpoint() -> Option<String> {
        let location = web_sys::window()?.location();
        let protocol = location.protocol().ok()?;
        let host = location.host().ok()?;
        let scheme = if protocol.starts_with("https") {
            "wss"
        } else {
            "ws"
        };
        Some(format!("{scheme}://{host}/api/ws"))
    }

    pub fn connect(
        on_message: impl Fn(ServerMessage) + 'static,
        on_status: impl Fn(Status) + 'static,
    ) {
        let Some(url) = endpoint() else {
            on_status(Status::Closed);
            return;
        };
        let Ok(socket) = WebSocket::new(&url) else {
            on_status(Status::Closed);
            return;
        };
        on_status(Status::Connecting);

        let on_status = std::rc::Rc::new(on_status);

        let on_open = {
            let on_status = on_status.clone();
            Closure::<dyn FnMut()>::new(move || on_status(Status::Open))
        };
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            let Some(text) = event.data().as_string() else {
                return;
            };
            match serde_json::from_str::<ServerMessage>(&text) {
                Ok(message) => on_message(message),
                // A message we cannot read means the bundle and the server
                // disagree about the protocol. Nothing useful to do but ignore it.
                Err(error) => tracing_stub(&format!("unreadable server message: {error}")),
            }
        });
        let on_close = {
            let on_status = on_status.clone();
            Closure::<dyn FnMut()>::new(move || on_status(Status::Closed))
        };
        let on_error = {
            let on_status = on_status.clone();
            Closure::<dyn FnMut(ErrorEvent)>::new(move |_| on_status(Status::Closed))
        };

        socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        socket.set_onerror(Some(on_error.as_ref().unchecked_ref()));

        CONNECTION.with(|slot| {
            *slot.borrow_mut() = Some(Connection {
                socket,
                _on_open: on_open,
                _on_message: on_message,
                _on_close: on_close,
                _on_error: on_error,
            });
        });
    }

    pub fn send(message: &ClientMessage) -> bool {
        let Ok(json) = serde_json::to_string(message) else {
            return false;
        };
        CONNECTION.with(|slot| {
            slot.borrow()
                .as_ref()
                .filter(|c| c.socket.ready_state() == WebSocket::OPEN)
                .map(|c| c.socket.send_with_str(&json).is_ok())
                .unwrap_or(false)
        })
    }

    pub fn is_open() -> bool {
        CONNECTION.with(|slot| {
            slot.borrow()
                .as_ref()
                .is_some_and(|c| c.socket.ready_state() == WebSocket::OPEN)
        })
    }

    /// Logs to the browser console; `tracing` is not wired up in the client.
    fn tracing_stub(message: &str) {
        web_sys::console::warn_1(&message.into());
    }
}

#[cfg(not(feature = "hydrate"))]
mod browser {
    use super::{ClientMessage, ServerMessage, Status};

    pub fn connect(
        _on_message: impl Fn(ServerMessage) + 'static,
        _on_status: impl Fn(Status) + 'static,
    ) {
    }

    pub fn send(_message: &ClientMessage) -> bool {
        false
    }

    pub fn is_open() -> bool {
        false
    }
}

/// Opens the connection, reporting messages and status changes as they arrive.
pub fn connect(on_message: impl Fn(ServerMessage) + 'static, on_status: impl Fn(Status) + 'static) {
    browser::connect(on_message, on_status);
}

/// Sends a message. Returns whether it went out.
pub fn send(message: &ClientMessage) -> bool {
    browser::send(message)
}

/// Whether the connection is usable right now.
pub fn is_open() -> bool {
    browser::is_open()
}

/// Milliseconds since the page loaded, with sub-millisecond resolution.
///
/// `performance.now()` rather than `Date.now()`: it is monotonic, so a clock
/// adjustment mid-exercise cannot produce a negative keystroke interval.
#[cfg(feature = "hydrate")]
pub fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}

/// Always zero without a browser.
#[cfg(not(feature = "hydrate"))]
pub fn now_ms() -> f64 {
    0.0
}

/// A random seed for an exercise.
#[cfg(feature = "hydrate")]
pub fn random_seed() -> u64 {
    let mut bytes = [0u8; 8];
    let filled = web_sys::window()
        .and_then(|w| w.crypto().ok())
        .map(|crypto| crypto.get_random_values_with_u8_array(&mut bytes).is_ok())
        .unwrap_or(false);
    if filled {
        u64::from_le_bytes(bytes)
    } else {
        // No crypto available: the clock is a poor seed but a working one, and an
        // exercise is not a secret.
        now_ms().to_bits()
    }
}

/// A fixed seed without a browser; server rendering shows no exercise anyway.
#[cfg(not(feature = "hydrate"))]
pub fn random_seed() -> u64 {
    0
}
