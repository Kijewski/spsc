#![allow(clippy::unwrap_used)] // it's okay to use `.unwrap()` in tests

use std::pin::{Pin, pin};
use std::task::{Context, Poll, Waker};

use crate::{RecvError, SendError, channel};

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
    }

    mod in_pollster {
        // nothing yet
    }
}

#[test]
fn no_panics_when_polled_after_completion() {
    let (sender, receiver) = channel();
    let mut receiver = pin!(receiver);
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
    let (sender, receiver) = channel();
    let mut receiver = pin!(receiver);
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
