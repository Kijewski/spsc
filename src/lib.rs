// SPDX-FileCopyrightText: 2026 René Kijewski <crates.io@k6i.de>
// SPDX-License-Identifier: ISC OR MIT OR Apache-2.0

#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! # spsc: single producer, single consumer for `no_std` Rust
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
//! * <https://spdx.org/licenses/ISC.html>,
//! * <https://spdx.org/licenses/MIT.html>, and
//! * <https://spdx.org/licenses/Apache-2.0.html>.

#[cfg(any(doc, test))]
extern crate std;

#[cfg(test)]
mod tests;

use core::convert::Infallible;
use core::error::Error;
use core::fmt;
use core::future::Future;
use core::mem::replace;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};

use bilock::{Bilock, Guard};

/// Returns an entangled <code>([Sender], [Receiver])</code> pair.
///
/// The [`Receiver`] is a [`Future`] that waits for a value to be sent by its [`Sender`].
#[must_use]
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let (sender, receiver) = Bilock::new(Inner {
        state: State::Alive,
        waker: None,
    });
    (
        Sender {
            holder: Holder(sender),
        },
        Receiver {
            holder: Some(Holder(receiver)),
        },
    )
}

/// Call [`send()`][Sender::send] to send a value to [`Receiver`].
pub struct Sender<T> {
    holder: Holder<T>,
}

/// Waits for the [`Sender`] to send a value.
///
/// This [`Future`] is drop-safe.
///
/// # Errors
///
/// The [`Future`] returns <code>[Err]\([RecvError])</code> if the entangled [`Sender`]
/// was dropped without sending a value.
pub struct Receiver<T> {
    holder: Option<Holder<T>>,
}

/// An error returned from [`Sender::send()`].
///
/// The entangled [`Receiver`] was already dropped.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SendError<T>(
    /// The `value` argument of [`Sender::send()`].
    pub T,
);

/// An error returned from <code>[receiver][Receiver].await</code>.
///
/// The entangled [`Sender`] was already dropped without sending a value, or a value
/// was already received.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RecvError;

/// An error returned from [`try_recv()`][Receiver::try_recv].
///
/// Either the entangled [`Sender`] was already dropped without sending a value,
/// a value was already received, or no value is ready, yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TryRecvError {
    /// No value was sent, yet.
    Empty,
    /// The [`Sender`] was dropped without sending a value, or a value was already received.
    Disconnected,
}

struct Holder<T>(Bilock<Inner<T>>);

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

impl fmt::Display for TryRecvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self {
            Self::Empty => "receiving on an empty channel",
            Self::Disconnected => "receiving on a closed channel",
        })
    }
}

impl<T> Error for SendError<T> {}

impl Error for RecvError {}

impl Error for TryRecvError {}

impl<T> From<Infallible> for SendError<T> {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

impl From<Infallible> for RecvError {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

impl From<Infallible> for TryRecvError {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

impl<T> Sender<T> {
    /// Consumes `Sender` and transmits `value` to [`Receiver`].
    ///
    /// # Errors
    ///
    /// Returns <code>[Err]\([SendError]\(value))</code> if the [`Receiver`] was dropped.
    pub fn send(mut self, value: T) -> Result<(), SendError<T>> {
        let mut guard = self.holder.0.lock();
        if let State::Alive = guard.state {
            guard.state = State::Value(value);
            take_waker_and_wake(guard);
            Ok(())
        } else {
            Err(SendError(value))
        }
    }
}

impl<T> Receiver<T> {
    /// Synchronously try to receive a value, or return immediately.
    ///
    /// # Errors
    ///
    /// Returns <code>[Err]\([TryRecvError]::Empty)</code> if no value was sent by the [`Sender`],
    /// yet, or <code>Err(TryRecvError::Disconnected)</code> if the Sender was dropped before
    /// sending a value.
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let Some(guard) = self
            .holder
            .as_mut()
            .ok_or(TryRecvError::Disconnected)?
            .0
            .try_lock()
        else {
            return Err(TryRecvError::Empty);
        };

        if let State::Alive = guard.state {
            Err(TryRecvError::Empty)
        } else {
            let value = take_value_waker_and_wake(guard);
            self.holder = None;
            value.ok_or(TryRecvError::Disconnected)
        }
    }
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, RecvError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Some(holder) = self.holder.as_mut() else {
            // Poll after completion ... We should probably panic, but simply returning that
            // the sender was dropped is not entirely wrong, either.
            return Poll::Ready(Err(RecvError));
        };
        let Some(mut guard) = holder.0.try_lock() else {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        };

        if let State::Alive = guard.state {
            let old_waker = if let Some(old_waker) = guard.waker.take() {
                if old_waker.will_wake(cx.waker()) {
                    guard.waker = Some(old_waker);
                    None
                } else {
                    guard.waker = Some(cx.waker().clone());
                    Some(old_waker)
                }
            } else {
                guard.waker = Some(cx.waker().clone());
                None
            };

            wake(old_waker, guard);
            Poll::Pending
        } else {
            let value = take_value_waker_and_wake(guard);
            self.holder = None;
            Poll::Ready(value.ok_or(RecvError))
        }
    }
}

impl<T> Drop for Holder<T> {
    fn drop(&mut self) {
        let mut guard = self.0.lock();
        if let State::Alive = guard.state {
            guard.state = State::Dead;
        }
        take_waker_and_wake(guard);
    }
}

fn take_value_waker_and_wake<T>(mut guard: Guard<'_, Inner<T>>) -> Option<T> {
    let result = if let State::Value(value) = replace(&mut guard.state, State::Dead) {
        Some(value)
    } else {
        None
    };
    take_waker_and_wake(guard);
    result
}

fn take_waker_and_wake<T>(mut guard: Guard<'_, Inner<T>>) {
    wake(guard.waker.take(), guard);
}

fn wake<T>(waker: Option<Waker>, guard: Guard<'_, Inner<T>>) {
    // Drop the lock before waking the waiting thread.
    // Otherwise it might get sent back to sleep immediately.
    drop(guard);

    if let Some(waker) = waker {
        waker.wake();
    }
}
