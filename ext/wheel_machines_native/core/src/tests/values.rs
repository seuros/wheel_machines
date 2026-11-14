//! Values are valid or do not exist; honoured answers record the clock only
//! when they depend on it.

use super::*;

#[test]
fn a_drain_whose_work_does_not_fit_a_usize_does_not_exist() {
    let drain = Drain::new(2, 3, Status::BudgetSpent);
    assert_eq!(
        drain.map(|d| (d.emitted(), d.cascaded(), d.status(), d.work())),
        Some((2, 3, Status::BudgetSpent, 5))
    );
    assert_eq!(drain.map(|d| d.is_caught_up()), Some(false));
    assert!(Drain::new(usize::MAX, 0, Status::OutputFull).is_some());
    assert!(Drain::new(0, usize::MAX, Status::CaughtUp).is_some());
    assert_eq!(Drain::new(usize::MAX, 1, Status::CaughtUp), None);
    assert_eq!(Drain::new(1, usize::MAX, Status::CaughtUp), None);
}

/// `advance` answers about the instant supplied, so it records it; a repeat
/// of the tick already held records the same value and changes nothing.
/// `register`, `cancel`, `drain` and `next_wake` take no instant and record
/// none: draining never moves the tick.
#[test]
fn only_advance_and_tick_record_the_clock() {
    let mut far = Wheel::<u64, 4>::new();
    honoured(far.register(1, 1_000_000));
    assert_eq!(far.drain(64, &mut [Due::default(); 4]).work(), 0);
    assert_eq!(far.next_wake(), Some(786_432));
    assert_eq!(far.now(), 0);
    assert_eq!(far.advance(5), Ok(()));
    assert!(far.drain(64, &mut [Due::default(); 4]).is_caught_up());
    assert_eq!(far.now(), 5);

    let mut wheel = Wheel::<u64, 4>::starting_at(0, 10);
    let handle = honoured(wheel.register(1, 50));
    let before = wheel.clone();
    assert_eq!(wheel.advance(10), Ok(()));
    assert_eq!(wheel, before);
    assert_eq!(wheel.drain(8, &mut [Due::default(); 4]).work(), 0);
    assert_eq!(wheel, before);
    assert_eq!(wheel.advance(20), Ok(()));
    assert_eq!(wheel.now(), 20);
    assert_eq!(wheel.cancel(handle), Ok(1));
    assert_eq!(wheel.now(), 20);
    assert_eq!(wheel.register(2, 20), Err(Refusal::Elapsed));
    assert_eq!(wheel.now(), 20);
}
