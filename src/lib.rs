/*!

A simple unbounded multi-producer multi-subscriber event bus built with crossbeam channels.

# Why double_decker?
Unlike the the `Bus` from the [`bus` crate](https://crates.io/crates/bus), `double_decker::Bus`
is unbounded and everyone knows that [double-decker buses](https://en.wikipedia.org/wiki/Double-decker_bus)
carry more passengers than a regular bus 🤷‍♂️.

Unlike `bus::Bus`, `double_decker::Bus` implements a cheap `Clone()` which I've found useful.

You should probably just use the [`bus`](https://crates.io/crates/bus) crate because it's mature and
completely lock-free.

# Design
`T` must implement `Clone` so it can be passed to all consumers.

When you call `add_rx()`, a `Sender<T>`/`Receiver<T>` pair are created and the `Sender` is
stored in a `HashMap` behind a `RwLock`.

`broadcast()` uses shared read access of the `RwLock` and sends out events to each `Receiver` in the
order they were added.

Lock contention can only occur when the number of subscribers changes as this requires write access to
the `RwLock`. This occurs when you call `add_rx()` or when you call `broadcast()` and one of more
of the `Sender`s returns `SendError` because it's disconnected.

# Examples plagiarised from `bus` crate

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
*/

use crossbeam::{bounded, unbounded, Receiver, Sender, TryRecvError};
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc, thread};

struct BusInner<T: Clone> {
    senders: HashMap<usize, Sender<T>>,
    next_id: usize,
}

impl<T: Clone> BusInner<T> {
    pub fn add_rx(&mut self) -> Receiver<T> {
        let (sender, receiver) = unbounded::<T>();
        self.senders.insert(self.next_id, sender);
        self.next_id += 1;
        receiver
    }

    pub fn broadcast(&self, event: T) -> Vec<usize> {
        let mut disconnected = Vec::with_capacity(0);

        if let Some(((last_id, last_sender), the_rest)) = self.get_sorted_senders().split_last() {
            for (id, sender) in the_rest.iter() {
                if sender.send(event.clone()).is_err() {
                    disconnected.push(**id);
                }
            }

            if last_sender.send(event).is_err() {
                disconnected.push(**last_id);
            };
        }

        disconnected
    }

    pub fn remove_senders(&mut self, ids: &[usize]) {
        for id in ids {
            self.senders.remove(&id);
        }
    }

    fn get_sorted_senders(&self) -> Vec<(&usize, &Sender<T>)> {
        let mut senders = self.senders.iter().collect::<Vec<(&usize, &Sender<T>)>>();
        senders.sort_by_key(|(id, _)| **id);
        senders
    }
}

impl<T: Clone> Default for BusInner<T> {
    fn default() -> Self {
        BusInner {
            senders: Default::default(),
            next_id: 0,
        }
    }
}

#[derive(Clone)]
pub struct Bus<T: Clone> {
    inner: Arc<RwLock<BusInner<T>>>,
}

impl<T: Clone> Bus<T> {
    /// Creates a new `double_decker::Bus`
    pub fn new() -> Self {
        Bus {
            inner: Default::default(),
        }
    }

    /// Adds a new `Receiver<T>`
    pub fn add_rx(&self) -> Receiver<T> {
        self.inner.write().add_rx()
    }

    /// Broadcast to all `Receiver`s
    pub fn broadcast(&self, event: T) {
        let disconnected = { self.inner.read().broadcast(event) };

        if disconnected.len() > 0 {
            self.inner.write().remove_senders(&disconnected);
        }
    }
}

impl<T: Clone> Default for Bus<T> {
    fn default() -> Self {
        Bus::new()
    }
}

type BoxedFn<T> = Box<dyn FnMut(T) + Send>;

#[derive(Clone)]
pub struct Subscription {
    terminate: Sender<()>,
}

impl Subscription {
    pub fn new(terminate: Sender<()>) -> Self {
        Subscription { terminate }
    }

    pub fn dispose(&self) {
        let _ = self.terminate.send(());
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.dispose();
    }
}

pub trait SubscribeOnThread<T: Send + 'static> {
    #[must_use]
    fn subscribe_on_thread(&self, callback: BoxedFn<T>) -> Subscription;
    fn subscribe(&self, callback: BoxedFn<T>);
}

impl<T: Send + 'static> SubscribeOnThread<T> for Receiver<T> {
    #[must_use]
    fn subscribe_on_thread(&self, mut callback: BoxedFn<T>) -> Subscription {
        let (terminate_tx, terminate_rx) = bounded::<()>(0);
        let receiver = self.clone();

        thread::Builder::new()
            .name("Receiver subscription thread".to_string())
            .spawn(move || loop {
                for event in receiver.try_iter() {
                    callback(event);
                }

                match terminate_rx.try_recv() {
                    Err(TryRecvError::Empty) => {}
                    _ => return,
                }
            })
            .expect("Could not start Receiver subscription thread");

        Subscription::new(terminate_tx)
    }

    fn subscribe(&self, mut callback: BoxedFn<T>) {
        for event in self.iter() {
            callback(event);
        }
    }
}

impl<T: Clone + Send + 'static> SubscribeOnThread<T> for Bus<T> {
    #[must_use]
    fn subscribe_on_thread(&self, callback: BoxedFn<T>) -> Subscription {
        self.add_rx().subscribe_on_thread(callback)
    }

    fn subscribe(&self, callback: BoxedFn<T>) {
        self.add_rx().subscribe(callback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam::RecvTimeoutError;
    use std::time::Duration;

    #[derive(Clone, PartialEq, Debug)]
    struct Something;

    #[derive(Clone, PartialEq, Debug)]
    enum Event {
        Start,
        Stop(Vec<Something>),
    }

    #[test]
    fn subscribe_on_thread() {
        let dispatcher = Bus::<Event>::new();

        // Ensure multiple subscriptions work
        let _sub_unused = dispatcher.subscribe_on_thread(Box::new(move |_event| {
            // But do nothing
        }));

        let __sub_unused = dispatcher.subscribe_on_thread(Box::new(move |_event| {
            // But do nothing
        }));

        let (tx_test, rx_test) = unbounded::<Event>();

        {
            let _sub = dispatcher.subscribe_on_thread(Box::new(move |event| {
                tx_test.send(event).unwrap();
            }));

            dispatcher.broadcast(Event::Start);
            dispatcher.broadcast(Event::Stop(vec![Something {}]));

            match rx_test.recv_timeout(Duration::from_millis(100)) {
                Err(_) => panic!("Event not received"),
                Ok(e) => assert_eq!(e, Event::Start),
            }

            match rx_test.recv_timeout(Duration::from_millis(100)) {
                Err(_) => panic!("Event not received"),
                Ok(e) => assert_eq!(e, Event::Stop(vec![Something {}])),
            }

            // _sub is dropped here
        }

        dispatcher.broadcast(Event::Start);

        match rx_test.recv_timeout(Duration::from_millis(100)) {
            Err(RecvTimeoutError::Disconnected) => {}
            _ => panic!("Subscription has been dropped so we should not get any events"),
        }
    }
}
