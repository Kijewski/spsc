# spsc: single producer, single consumer for `no_std` Rust

[![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/Kijewski/spsc/ci.yml?branch=main&style=flat-square&logo=github&logoColor=white "GitHub Workflow Status")](https://github.com/Kijewski/spsc/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/spsc?logo=rust&style=flat-square "Crates.io")](https://crates.io/crates/spsc)
[![docs.rs](https://img.shields.io/docsrs/spsc?logo=docsdotrs&style=flat-square&logoColor=white "docs.rs")](https://docs.rs/spsc/)

An entangled sync sender + async receiver pair.
A minimalistic, runtime-agnostic implementation.

Have a look at [`oneshot`](https://lib.rs/crates/oneshot) if you need a
more-complete / more-complex / more-well tested implementation.

## Example

```rust
let (user_sender, user_receiver) = spsc::channel();

// You can use any runtime you like:
// tokio, smol, pollster, ... it works with everything!
let rx = tokio::spawn(async {
    // Receiving is async:
    // the call only returns once a value was sent,
    // or the sender was dropped.
    let user = user_receiver.await.unwrap();
    assert_eq!(user, "Max Mustermann");
});

// Sending is synchronous:
// The call does not need to happen inside a runtime.
user_sender.send("Max Mustermann").unwrap();

rx.await.unwrap();
```

## License

This project is tri-licensed under <tt>ISC OR MIT OR Apache-2.0</tt>.
Contributions must be licensed under the same terms.
Users may follow any one of these licenses, or all of them.

See the individual license texts at
* <https://spdx.org/licenses/ISC.html>,
* <https://spdx.org/licenses/MIT.html>, and
* <https://spdx.org/licenses/Apache-2.0.html>.
