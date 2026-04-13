// SPDX-FileCopyrightText: 2026 René Kijewski <crates.io@k6i.de>
// SPDX-License-Identifier: ISC OR MIT OR Apache-2.0

//! # spsc: single producer, single consumer
//!
//! [![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/Kijewski/spsc/ci.yml?branch=main&style=flat-square&logo=github&logoColor=white "GitHub Workflow Status")](https://github.com/Kijewski/spsc/actions/workflows/ci.yml)
//! [![Crates.io](https://img.shields.io/crates/v/spsc?logo=rust&style=flat-square "Crates.io")](https://crates.io/crates/spsc)
//! [![docs.rs](https://img.shields.io/docsrs/spsc?logo=docsdotrs&style=flat-square&logoColor=white "docs.rs")](https://docs.rs/spsc/)
//!
//! An entangled sync sender + async receiver pair.
//! A minimalistic, runtime-agnostic implementation.
//!
//! Have a look at [`oneshot`](https://lib.rs/crates/oneshot) if you need a
//! more-complete / more-complex / more-well tested implementation.
//!
//! ## Example
//!
//! ```
//! # tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
//! let (user_sender, user_receiver) = spsc::channel();
//!
//! // You can use any runtime you like:
//! // tokio, smol, pollster, ... it works with everything!
//! let rx = tokio::spawn(async {
//!     // Receiving is async:
//!     // the call only returns once a value was sent,
//!     // or the sender was dropped.
//!     let user = user_receiver.await.unwrap();
//!     assert_eq!(user, "Max Mustermann");
//! });
//!
//! // Sending is synchronous:
//! // The call does not need to happen inside a runtime.
//! user_sender.send("Max Mustermann").unwrap();
//!
//! rx.await.unwrap();
//! # });
//! ```
//!
//! ## License
//!
//! This project is tri-licensed under <tt>ISC OR MIT OR Apache-2.0</tt>.
//! Contributions must be licensed under the same terms.
//! Users may follow any one of these licenses, or all of them.
//!
//! See the individual license texts at
//! * <https://choosealicense.com/licenses/isc/>,
//! * <https://choosealicense.com/licenses/mit/>, and
//! * <https://choosealicense.com/licenses/apache-2.0/>.

#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(test)]
mod tests;

use std::error::Error;
use std::fmt;
use std::future::Future;
use std::mem::replace;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use pin_project_lite::pin_project;

/// Returns an entangled <code>([Sender], [Receiver])</code> pair.
///
/// The [`Receiver`] is a [`Future`] that waits for a value to be sent by its [`Sender`].
#[must_use]
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let holder = Holder(Some(Arc::new(Mutex::new(Inner {
        state: State::Alive,
        waker: None,
    }))));

    (
        Sender {
            holder: holder.clone(),
        },
        Receiver { holder },
    )
}

/// Call [`send()`][Sender::send] to send a value to [`Receiver`].
pub struct Sender<T> {
    holder: Holder<T>,
}

pin_project! {
    /// Waits for the [`Sender`] to send a value.
    ///
    /// This [`Future`] is drop-safe.
    ///
    /// # Errors
    ///
    /// The [`Future`] returns <code>[`Err`]\([RecvError])</code> if the entangled [`Sender`]
    /// was dropped without sending a value.
    pub struct Receiver<T> {
        #[pin]
        holder: Holder<T>,
    }
}

/// An error returned from [`Sender::send()`].
///
/// The entangled [`Receiver`] was already dropped.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SendError<T>(pub T);

/// An error returned from <code>[receiver][Receiver].await</code>.
///
/// The entangled [`Sender`] was already dropped without sending a value.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RecvError;

// `pin_project!` does not allow to implement `Drop`, so this indirection is needed.
struct Holder<T>(Option<Arc<Mutex<Inner<T>>>>);

struct Inner<T> {
    state: State<T>,
    waker: Option<Waker>,
}

enum State<T> {
    /// The sender and receiver are still connected and no value has been sent.
    Alive,
    /// The sender or receiver has been dropped without sending a value.
    Dead,
    /// A value has been sent and is waiting for the receiver to consume it.
    Value(T),
}

impl<T> fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sender").finish_non_exhaustive()
    }
}

impl<T> fmt::Debug for Receiver<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Receiver").finish_non_exhaustive()
    }
}

impl<T> fmt::Debug for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SendError").finish_non_exhaustive()
    }
}

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("sending on a closed channel")
    }
}

impl fmt::Display for RecvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("receiving on a closed channel")
    }
}

impl<T> Error for SendError<T> {}

impl Error for RecvError {}

// `#[derive(Clone)]` will `impl Clone for Holder<T> where T: Clone`,
// but we only need `Arc<_>: Clone`, and `Arc<_>` is always `Clone`.
impl<T> Clone for Holder<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Sender<T> {
    /// Consumes `Sender` and transmits `value` to [`Receiver`].
    ///
    /// # Errors
    ///
    /// Returns <code>[Err]\([SendError]\(value))</code> if the [`Receiver`] was dropped.
    pub fn send(mut self, value: T) -> Result<(), SendError<T>> {
        let Some(inner) = self.holder.0.take() else {
            // impossible: `send()` called after sender was consumed
            return Err(SendError(value));
        };
        let inner = &mut *match inner.lock() {
            Ok(inner) => inner,
            Err(inner) => inner.into_inner(),
        };

        if !matches!(inner.state, State::Alive) {
            return Err(SendError(value));
        }

        inner.state = State::Value(value);
        if let Some(waker) = inner.waker.take() {
            waker.wake();
        }
        Ok(())
    }
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        let result = {
            // Please forgive me for the next line!
            let Some(inner) = this.holder.as_ref().get_ref().0.as_ref() else {
                // Poll after completion ... We should probably panic, but simply returning that
                // the sender was dropped is not entirely wrong, either.
                return Poll::Ready(Err(RecvError));
            };
            let inner = &mut *match inner.lock() {
                Ok(inner) => inner,
                Err(inner) => inner.into_inner(),
            };

            match replace(&mut inner.state, State::Dead) {
                State::Value(value) => Ok(value),
                State::Dead => Err(RecvError),
                State::Alive => {
                    inner.state = State::Alive;
                    if let Some(existing) = &inner.waker {
                        if !existing.will_wake(cx.waker()) {
                            inner.waker = Some(cx.waker().clone());
                        }
                    } else {
                        inner.waker = Some(cx.waker().clone());
                    }
                    return Poll::Pending;
                }
            }
        };
        this.holder.0 = None;
        Poll::Ready(result)
    }
}

impl<T> Drop for Holder<T> {
    fn drop(&mut self) {
        let Some(inner) = self.0.take() else {
            return;
        };
        let inner = &mut *match inner.lock() {
            Ok(inner) => inner,
            Err(inner) => inner.into_inner(),
        };

        if matches!(inner.state, State::Alive) {
            inner.state = State::Dead;
        }
        if let Some(waker) = inner.waker.take() {
            waker.wake();
        }
    }
}
