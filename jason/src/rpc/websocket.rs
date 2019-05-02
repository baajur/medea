//! ['WebSocket'](https://developer.mozilla.org/ru/docs/WebSockets)
//! transport wrapper.
use futures::future::{Future, IntoFuture};
use futures::stream::Stream;
use web_sys::{CloseEvent, Event, MessageEvent, WebSocket as BackingSocket};

use std::{cell::RefCell, convert::TryFrom, rc::Rc};

use crate::{
    rpc::{
        protocol::{ClientMsg, ServerMsg},
        CloseMsg,
    },
    utils::{EventListener, WasmErr},
};

#[derive(Debug)]
enum State {
    CONNECTING,
    OPEN,
    CLOSING,
    CLOSED,
}

impl State {
    pub fn can_close(&self) -> bool {
        match self {
            State::CONNECTING | State::OPEN => true,
            _ => false,
        }
    }
}

impl TryFrom<u16> for State {
    type Error = WasmErr;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(State::CONNECTING),
            1 => Ok(State::OPEN),
            2 => Ok(State::CLOSING),
            3 => Ok(State::CLOSED),
            _ => Err(WasmErr::Other(
                format!("Could not cast {} to State variant", value).into(),
            )),
        }
    }
}

struct InnerSocket {
    socket: Rc<BackingSocket>,
    socket_state: State,
    on_open: Option<EventListener<BackingSocket, Event>>,
    on_message: Option<EventListener<BackingSocket, MessageEvent>>,
    on_close: Option<EventListener<BackingSocket, CloseEvent>>,
    on_error: Option<EventListener<BackingSocket, Event>>,
}

pub struct WebSocket(Rc<RefCell<InnerSocket>>);

impl InnerSocket {
    fn new(url: &str) -> Result<Self, WasmErr> {
        Ok(Self {
            socket_state: State::CONNECTING,
            socket: Rc::new(BackingSocket::new(url)?),
            on_open: None,
            on_message: None,
            on_close: None,
            on_error: None,
        })
    }

    fn update_state(&mut self) {
        match State::try_from(self.socket.ready_state()) {
            Ok(new_state) => self.socket_state = new_state,
            Err(err) => {
                // unreachable, unless some vendor will break enum
                err.log_err()
            }
        };
    }
}

impl WebSocket {
    // Resolves only if connection succeeded.
    pub fn new(url: &str) -> impl Future<Item = Self, Error = WasmErr> {
        let (tx_error, rx_error) = futures::sync::mpsc::unbounded();
        let (tx_open, rx_open) = futures::oneshot();

        InnerSocket::new(url).into_future().and_then(move |socket| {
            let socket = Rc::new(RefCell::new(socket));
            let mut socket_mut = socket.borrow_mut();

            let inner = Rc::clone(&socket);
            socket_mut.on_error = Some(EventListener::new_mut(
                Rc::clone(&socket_mut.socket),
                "error",
                move |_| {
                    tx_error.unbounded_send(inner.borrow_mut().update_state()).map_err(|_|());
                },
            )?);

            let inner = Rc::clone(&socket);
            socket_mut.on_open = Some(EventListener::new_once(
                Rc::clone(&socket_mut.socket),
                "open",
                move |_| {
                    tx_open.send(inner.borrow_mut().update_state()).unwrap();
                },
            )?);

            drop(socket_mut);
            Ok(Self(socket))
        }).and_then(move |socket| {
            rx_open.then(move |_| Ok(socket))
                .select(rx_error.into_future().then(|_| Err(WasmErr::from_str("init socket error"))))
                // TODO try and_then
                .then(|res| match res {
                    Ok((s, _)) => Ok(s),
                    Err((e, _)) => Err(e),
                })
        })
    }

    pub fn on_message<F>(&self, mut f: F) -> Result<(), WasmErr>
    where
        F: (FnMut(Result<ServerMsg, WasmErr>)) + 'static,
    {
        let mut inner_mut = self.0.borrow_mut();
        inner_mut.on_message = Some(EventListener::new_mut(
            Rc::clone(&inner_mut.socket),
            "message",
            move |msg| {
                let parsed = ServerMsg::try_from(&msg);

                f(parsed);
            },
        )?);
        Ok(())
    }

    pub fn on_close<F>(&self, f: F) -> Result<(), WasmErr>
    where
        F: (FnOnce(CloseMsg)) + 'static,
    {
        let mut inner_mut = self.0.borrow_mut();
        let inner = Rc::clone(&self.0);
        inner_mut.on_close = Some(EventListener::new_once(
            Rc::clone(&inner_mut.socket),
            "close",
            move |msg: CloseEvent| {
                inner.borrow_mut().update_state();
                let parsed = CloseMsg::from(&msg);

                f(parsed);
            },
        )?);
        Ok(())
    }

    pub fn send(&self, msg: &ClientMsg) -> Result<(), WasmErr> {
        let inner = self.0.borrow();

        match inner.socket_state {
            State::OPEN => inner
                .socket
                .send_with_str(&serde_json::to_string(msg)?)
                .map_err(WasmErr::from),
            _ => Err(WasmErr::from_str("Underlying socket is closed")),
        }
    }
}

impl Drop for WebSocket {
    fn drop(&mut self) {
        let mut inner = self.0.borrow_mut();
        if inner.socket_state.can_close() {
            inner.on_open.take();
            inner.on_error.take();
            inner.on_message.take();
            inner.on_close.take();

            if let Err(err) = inner
                .socket
                .close_with_code_and_reason(1000, "Dropped unexpectedly")
            {
                WasmErr::from(err).log_err();
            }
        }
    }
}

impl TryFrom<&MessageEvent> for ServerMsg {
    type Error = WasmErr;

    fn try_from(msg: &MessageEvent) -> Result<Self, Self::Error> {
        let payload = msg
            .data()
            .as_string()
            .ok_or_else(|| WasmErr::from_str("Payload is not string"))?;

        serde_json::from_str::<Self>(&payload).map_err(WasmErr::from)
    }
}

impl From<&CloseEvent> for CloseMsg {
    fn from(event: &CloseEvent) -> Self {
        let code: u16 = event.code();
        let body = format!("{}:{}", code, event.reason());
        match code {
            1000 => CloseMsg::Normal(body),
            _ => CloseMsg::Disconnect(body),
        }
    }
}
