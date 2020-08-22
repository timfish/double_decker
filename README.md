# double_decker


A simple unbounded multi-producer multi-subscriber event bus built with crossbeam channels.

## Why double_decker?
Unlike the the `Bus` from the [`bus` crate](https://crates.io/crates/bus), `double_decker::Bus`
is unbounded and everyone knows that [double-decker buses](https://en.wikipedia.org/wiki/Double-decker_bus)
carry more passengers than a regular bus 🤷‍♂️.

Unlike `bus::Bus`, `double_decker::Bus` implements a cheap `Clone()` which I've found useful.

You should probably just use the [`bus`](https://crates.io/crates/bus) crate because it's mature and
completely lock-free.

## Design
`T` must implement `Clone` so it can be passed to all consumers.

When you call `add_rx()`, a `Sender<T>`/`Receiver<T>` pair are created and the `Sender` is
stored in a `HashMap` behind a `RwLock`.

`broadcast()` uses shared read access of the `RwLock` and sends out events to each `Receiver` in the
order they were added.

Lock contention can only occur when the number of subscribers changes as this requires write access to
the `RwLock`. This occurs when you call `add_rx()` or when you call `broadcast()` and one of more
of the `Sender`s returns `SendError` because it's disconnected.

## Examples plagiarised from `bus` crate

Single-send, multi-consumer example

```rust
use double_decker::Bus;

let mut bus = Bus::new();
let mut rx1 = bus.add_rx();
let mut rx2 = bus.add_rx();

bus.broadcast("Hello");
assert_eq!(rx1.recv(), Ok("Hello"));
assert_eq!(rx2.recv(), Ok("Hello"));
```

Multi-send, multi-consumer example

```rust
use double_decker::Bus;
use std::thread;

let mut bus = Bus::new();
let mut rx1 = bus.add_rx();
let mut rx2 = bus.add_rx();

// start a thread that sends 1..100
let j = thread::spawn(move || {
    for i in 1..100 {
        bus.broadcast(i);
    }
});

// every value should be received by both receivers
for i in 1..100 {
    // rx1
    assert_eq!(rx1.recv(), Ok(i));
    // and rx2
    assert_eq!(rx2.recv(), Ok(i));
}

j.join().unwrap();
```

Also included are `subscribe` and `subscribe_on_thread` which allow you to subscribe to broadcast
events with a closure that is called on every broadcast. `subscribe` is blocking whereas
`subscribe_on_thread` calls the closure from another thread.

`subscribe_on_thread` returns a `Subscription` which you should hang on to as the thread terminates
when this is dropped.

```rust
use double_decker::{Bus, SubscribeOnThread};

let bus = Bus::<i32>::new();

// This would block
// bus.subscribe(Box::new(move |_event| {
//     // This closure is called on every broadcast
// }));

let _subscription = bus.subscribe_on_thread(Box::new(move |_event| {
    // This closure is called on every broadcast
}));

bus.broadcast(5);
```


License: MIT
