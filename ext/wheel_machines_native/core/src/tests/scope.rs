//! Handles are scoped ids: only the wheel that issued one honours it.

use super::*;

/// The independent review's sequence: `a`'s handle cancelled `b`'s unrelated
/// registration, because both wheels issued slot 0, generation 1.
#[test]
fn a_handle_from_another_scope_is_refused_and_changes_nothing() {
    let mut a = Wheel::<u64, 1>::with_scope(1);
    let mut b = Wheel::<u64, 1>::with_scope(2);
    assert_eq!((a.scope(), b.scope()), (1, 2));
    let from_a = registered(a.register(1, 10));
    let from_b = registered(b.register(2, 10));
    assert_eq!((from_a.scope(), from_b.scope()), (1, 2));
    assert_eq!(from_a.slot, from_b.slot);
    assert_eq!(sequence_of(from_a.key), sequence_of(from_b.key));
    let (a_before, b_before) = (a.clone(), b.clone());

    assert_eq!(b.cancel(from_a), Err(Refusal::Stale));
    assert_eq!(b.deadline(from_a), None);
    assert_eq!(b, b_before);
    assert_eq!(a.cancel(from_b), Err(Refusal::Stale));
    assert_eq!(a.deadline(from_b), None);
    assert_eq!(a, a_before);

    assert_eq!(
        (a.deadline(from_a), b.deadline(from_b)),
        (Some(10), Some(10))
    );
    assert_eq!((a.cancel(from_a), b.cancel(from_b)), (Ok(1), Ok(2)));
}

/// Scopes are the caller's: the core reads no clock and no random source, so
/// two wheels left in one scope issue the same handles and still alias.
#[test]
fn two_wheels_left_in_the_same_scope_still_alias() {
    let mut a = Wheel::<u64, 1>::new();
    let mut b = Wheel::<u64, 1>::new();
    let from_a = registered(a.register(1, 10));
    let from_b = registered(b.register(2, 10));
    assert_eq!(from_a, from_b);
    assert_eq!(b.cancel(from_a), Ok(2));
    assert!(b.is_empty());
}

#[test]
fn every_constructor_carries_its_scope_and_scope_zero_counts_from_one() {
    assert_eq!(Wheel::<u64, 2>::new(), Wheel::with_scope(0));
    assert_eq!(Wheel::<u64, 2>::default(), Wheel::with_scope(0));
    assert_eq!(Wheel::<u64, 2>::with_scope(7), Wheel::starting_at(7, 0));
    assert_eq!(
        Wheel::<u64, 2>::resumed(7, 0, 9),
        Some(Wheel::starting_at(7, 9))
    );
    assert_ne!(Wheel::<u64, 2>::with_scope(7), Wheel::with_scope(8));

    let mut plain = Wheel::<u64, 2>::new();
    assert_eq!(plain.scope(), 0);
    assert_eq!(plain.register(1, 10), Ok(Handle { slot: 0, key: 1 }));
    assert_eq!(plain.register(2, 10), Ok(Handle { slot: 1, key: 2 }));

    let mut scoped_wheel = Wheel::<u64, 2>::starting_at(7, 100);
    assert_eq!((scoped_wheel.scope(), scoped_wheel.now()), (7, 100));
    let first = registered(scoped_wheel.register(1, 101));
    assert_eq!(
        first,
        Handle {
            slot: 0,
            key: scoped(7, 1)
        }
    );
    assert_eq!(first.scope(), 7);
}

#[test]
fn a_handle_of_this_scope_that_was_never_issued_is_stale() {
    let mut wheel = Wheel::<u64, 2>::with_scope(3);
    let live = registered(wheel.register(1, 10));
    let before = wheel.clone();
    let unissued = Handle {
        slot: live.slot,
        key: scoped(3, 2),
    };
    assert_eq!(wheel.cancel(unissued), Err(Refusal::Stale));
    assert_eq!(wheel.deadline(unissued), None);
    let none = Handle {
        slot: live.slot,
        key: 0,
    };
    assert_eq!(wheel.cancel(none), Err(Refusal::Stale));
    assert_eq!(wheel, before);
}

#[test]
fn the_last_id_of_a_scope_is_issued_and_then_the_wheel_is_exhausted() {
    let mut wheel = resumed::<2>(3, SEQUENCE_MAX - 1, 0);
    let before = wheel.clone();
    // A call refused for another reason spends no id.
    assert_eq!(wheel.register(1, 0), Err(Refusal::Elapsed));
    assert_eq!(wheel, before);
    let last = registered(wheel.register(1, 10));
    assert_eq!(
        last,
        Handle {
            slot: 0,
            key: scoped(3, SEQUENCE_MAX)
        }
    );
    assert_eq!(last.scope(), 3);
    let before = wheel.clone();
    assert_eq!(wheel.register(2, 10), Err(Refusal::Exhausted));
    assert_eq!(wheel, before);
    // The last handle still works; its id is never reissued.
    assert_eq!(wheel.cancel(last), Ok(1));
    assert_eq!(wheel.register(2, 10), Err(Refusal::Exhausted));
    assert_eq!(wheel.cancel(last), Err(Refusal::Stale));
}

/// A fresh wheel would count from one again, and a handle that outlived the
/// restart would name the new registration filed in its entry.
#[test]
fn a_resumed_wheel_continues_its_scope_and_refuses_every_earlier_handle() {
    let mut first = Wheel::<u64, 2>::starting_at(5, 100);
    let old: [Handle; 3] = core::array::from_fn(|at| {
        let handle = registered(first.register(at as u64, 200));
        assert_eq!(first.cancel(handle).map(|_| ()), Ok(()));
        handle
    });
    let pending = registered(first.register(9, 300));
    assert_eq!(first.issued(), 4);

    let mut wheel = resumed::<2>(first.scope(), first.issued(), first.now());
    assert_eq!((wheel.scope(), wheel.issued(), wheel.now()), (5, 4, 100));
    // Only the sequence came back: nothing is registered.
    assert!(wheel.is_empty());
    assert_eq!(wheel.next_wake(), None);
    let next = registered(wheel.register(7, 150));
    assert_eq!(next.key, scoped(5, 5));
    // The entry every earlier handle sat in is in use again.
    assert_eq!((next.slot, pending.slot), (old[0].slot, old[0].slot));
    let again = registered(wheel.register(8, 150));
    assert_eq!(again.key, scoped(5, 6));

    let before = wheel.clone();
    for handle in old.into_iter().chain([pending]) {
        assert_ne!(handle.key, next.key);
        assert_ne!(handle.key, again.key);
        assert_eq!(wheel.cancel(handle), Err(Refusal::Stale));
        assert_eq!(wheel.deadline(handle), None);
    }
    assert_eq!(wheel, before);
    assert_eq!((wheel.cancel(next), wheel.cancel(again)), (Ok(7), Ok(8)));
}

#[test]
fn a_wheel_resumed_at_the_last_sequence_is_spent_and_one_past_it_is_none() {
    let mut spent = resumed::<2>(3, SEQUENCE_MAX, 0);
    assert_eq!((spent.scope(), spent.issued()), (3, SEQUENCE_MAX));
    let before = spent.clone();
    assert_eq!(spent.register(1, 10), Err(Refusal::Exhausted));
    assert_eq!(spent, before);
    assert_eq!(Wheel::<u64, 2>::resumed(3, SEQUENCE_MAX + 1, 0), None);
    assert_eq!(Wheel::<u64, 2>::resumed(3, u64::MAX, 0), None);
}
