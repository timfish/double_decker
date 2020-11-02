use crossbeam::channel::TryRecvError;

// These tests were plagiarised from the bus crate!

#[test]
fn it_works() {
    let c = double_decker::Bus::new();
    let r1 = c.add_rx();
    let r2 = c.add_rx();
    c.broadcast(true);

    assert_eq!(r1.try_recv(), Ok(true));
    assert_eq!(r2.try_recv(), Ok(true));
}
#[test]
fn it_fails_when_empty() {
    let c = double_decker::Bus::<bool>::new();
    let r1 = c.add_rx();
    assert_eq!(r1.try_recv(), Err(TryRecvError::Empty));
}

#[test]
fn it_iterates() {
    use std::thread;

    let tx = double_decker::Bus::new();
    let rx = tx.add_rx();
    let j = thread::spawn(move || {
        for i in 0..1_000 {
            tx.broadcast(i);
        }
    });

    let mut ii = 0;
    for i in rx.iter() {
        assert_eq!(i, ii);
        ii += 1;
    }

    j.join().unwrap();
    assert_eq!(ii, 1_000);
    assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
}

#[test]
fn aggressive_iteration() {
    for _ in 0..1_000 {
        use std::thread;

        let tx = double_decker::Bus::new();
        let rx = tx.add_rx();
        let j = thread::spawn(move || {
            for i in 0..1_000 {
                tx.broadcast(i);
            }
        });

        let mut ii = 0;
        for i in rx.iter() {
            assert_eq!(i, ii);
            ii += 1;
        }

        j.join().unwrap();
        assert_eq!(ii, 1_000);
        assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
    }
}

#[test]
fn it_detects_closure() {
    let tx = double_decker::Bus::new();
    let rx = tx.add_rx();
    tx.broadcast(true);

    assert_eq!(rx.try_recv(), Ok(true));
    assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
    drop(tx);
    assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
}

#[test]
fn it_recvs_after_close() {
    let tx = double_decker::Bus::new();
    let rx = tx.add_rx();
    tx.broadcast(true);

    drop(tx);
    assert_eq!(rx.try_recv(), Ok(true));
    assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
}

#[test]
fn it_handles_leaves() {
    let c = double_decker::Bus::new();
    let r1 = c.add_rx();
    let r2 = c.add_rx();
    c.broadcast(true);

    drop(r2);
    assert_eq!(r1.try_recv(), Ok(true));
}

#[test]
fn it_runs_blocked_reads() {
    use std::thread;

    let tx = Box::new(double_decker::Bus::new());
    let rx = tx.add_rx();
    // buffer is now empty
    assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
    // start other thread that blocks
    let c = thread::spawn(move || {
        rx.recv().unwrap();
    });
    // unblock receiver by broadcasting
    tx.broadcast(true);
    // check that thread now finished
    c.join().unwrap();
}

#[test]
fn it_can_count_to_10000() {
    use std::thread;

    let c = double_decker::Bus::new();
    let r1 = c.add_rx();
    let j = thread::spawn(move || {
        for i in 0..10_000 {
            c.broadcast(i);
        }
    });

    for i in 0..10_000 {
        assert_eq!(r1.recv(), Ok(i));
    }

    j.join().unwrap();
    assert_eq!(r1.try_recv(), Err(TryRecvError::Disconnected));
}
