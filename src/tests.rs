#![allow(clippy::unwrap_used)] // it's okay to use `.unwrap()` in tests

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use tokio::time::timeout;

use crate::{RecvError, SendError, TryRecvError, channel};

macro_rules! test_in_multiple_runtimes {
    (
        $(mod in_any {
            $(async fn $any_name:ident() $any_block:block)*
        })?

        $(mod in_tokio {
            $(async fn $tokio_name:ident() $tokio_block:block)*
        })?

        $(mod in_pollster {
            $(async fn $pollster_name:ident() $pollster_block:block)*
        })?

        $(mod in_smol {
            $(async fn $smol_name:ident() $smol_block:block)*
        })?
    ) => {
        test_in_multiple_runtimes! {
            @

            mod in_tokio {
                $($(async fn $any_name() $any_block)*)?
                $($(async fn $tokio_name() $tokio_block)*)?
            }

            mod in_pollster {
                $($(async fn $any_name() $any_block)*)?
                $($(async fn $pollster_name() $pollster_block)*)?
            }

            mod in_smol {
                $($(async fn $any_name() $any_block)*)?
                $($(async fn $smol_name() $smol_block)*)?
            }
        }
    };

    (
        @

        mod in_tokio {
            $(async fn $tokio_name:ident() $tokio_block:block)*
        }

        mod in_pollster {
            $(async fn $pollster_name:ident() $pollster_block:block)*
        }

        mod in_smol {
            $(async fn $smol_name:ident() $smol_block:block)*
        }
    ) => {
        mod in_tokio {
            use super::*;

            $(
                #[tokio::test]
                async fn $tokio_name() $tokio_block
            )*
        }

        mod in_pollster {
            use super::*;

            $(
                #[test]
                fn $pollster_name() {
                    pollster::block_on(async $pollster_block)
                }
            )*
        }

        mod in_smol {
            use super::*;

            $(
                #[test]
                fn $smol_name() {
                    smol::block_on(async $smol_block)
                }
            )*
        }
    };
}

test_in_multiple_runtimes! {
    mod in_any {
        async fn receiver_gets_sent_value() {
            let (sender, receiver) = channel();

            assert_eq!(sender.send(42), Ok(()));
            assert_eq!(receiver.await, Ok(42));
        }

        async fn receiver_returns_error_when_sender_dropped() {
            let (sender, receiver) = channel::<i32>();
            drop(sender);

            assert_eq!(receiver.await, Err(RecvError));
        }

        async fn sender_returns_error_when_receiver_dropped() {
            let (sender, receiver) = channel();
            drop(receiver);

            assert_eq!(sender.send(99), Err(SendError(99)));
        }
    }

    mod in_tokio {
        async fn receiver_waits_for_send() {
            let (sender, receiver) = channel();

            let receiver = tokio::spawn(receiver);
            assert!(!receiver.is_finished());
            assert_eq!(sender.send(7), Ok(()));
            assert_eq!(receiver.await.unwrap(), Ok(7));
        }

        async fn drop_safety() {
            let (sender, mut receiver) = channel();

            assert!(timeout(Duration::from_millis(10), &mut receiver).await.is_err());
            assert_eq!(sender.send(123), Ok(()));
            assert_eq!(
                timeout(Duration::from_millis(10), &mut receiver).await,
                Ok(Ok(123))
            );
            assert_eq!(
                timeout(Duration::from_millis(10), &mut receiver).await,
                Ok(Err(RecvError))
            );
        }
    }

    mod in_pollster {
        // nothing yet
    }

    mod in_smol {
        async fn receiver_waits_for_send() {
            let (sender, receiver) = channel();

            let receiver = smol::spawn(receiver);
            assert!(!receiver.is_finished());
            assert_eq!(sender.send(7), Ok(()));
            assert_eq!(receiver.await, Ok(7));
        }
    }
}

#[test]
fn no_panics_when_polled_after_completion() {
    let (sender, mut receiver) = channel();
    let mut cx = Context::from_waker(Waker::noop());

    assert_eq!(Pin::new(&mut receiver).poll(&mut cx), Poll::Pending);
    assert_eq!(sender.send(13), Ok(()));
    assert_eq!(Pin::new(&mut receiver).poll(&mut cx), Poll::Ready(Ok(13)));
    assert_eq!(
        Pin::new(&mut receiver).poll(&mut cx),
        Poll::Ready(Err(RecvError))
    );
}

#[test]
fn no_panics_when_polled_after_completion_times_two() {
    let (sender, mut receiver) = channel();
    let mut cx = Context::from_waker(Waker::noop());

    assert_eq!(Pin::new(&mut receiver).poll(&mut cx), Poll::Pending);
    assert_eq!(Pin::new(&mut receiver).poll(&mut cx), Poll::Pending);

    assert_eq!(sender.send(13), Ok(()));
    assert_eq!(Pin::new(&mut receiver).poll(&mut cx), Poll::Ready(Ok(13)));

    assert_eq!(
        Pin::new(&mut receiver).poll(&mut cx),
        Poll::Ready(Err(RecvError))
    );
    assert_eq!(
        Pin::new(&mut receiver).poll(&mut cx),
        Poll::Ready(Err(RecvError))
    );
}

#[test]
#[allow(clippy::bool_assert_comparison)] // more consistent, and easier on the eyes than `!(!..)`
fn send_and_try_recv() {
    let (sender, mut receiver) = channel();

    let woken = Arc::new(AtomicBool::new(false));
    let waker = waker_fn::waker_fn({
        let woken = Arc::clone(&woken);
        move || woken.store(true, SeqCst)
    });
    let mut cx = Context::from_waker(&waker);

    assert_eq!(Pin::new(&mut receiver).poll(&mut cx), Poll::Pending);
    assert_eq!(woken.load(SeqCst), false);
    assert_eq!(receiver.try_recv(), Err(TryRecvError::Empty));
    assert_eq!(woken.load(SeqCst), false);
    assert_eq!(sender.send(37845), Ok(()));
    assert_eq!(woken.load(SeqCst), true);
    assert_eq!(receiver.try_recv(), Ok(37845));
    assert_eq!(receiver.try_recv(), Err(TryRecvError::Disconnected));
    assert_eq!(
        Pin::new(&mut receiver).poll(&mut cx),
        Poll::Ready(Err(RecvError))
    );
}
