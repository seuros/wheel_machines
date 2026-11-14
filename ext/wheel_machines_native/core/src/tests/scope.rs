//! Handles are scoped ids: only the wheel that issued one honours it.

use super::*;

/// The independent review's sequence: `a`'s handle cancelled `b`'s unrelated
/// registration, because both wheels issued slot 0, generation 1.
#[test]
fn a_handle_from_another_scope_is_refused_and_changes_nothing() {
    let mut a = Wheel::<u64, 1>::with_scope(1);
    let mut b = Wheel::<u64, 1>::with_scope(2);
    assert_eq!((a.scope(), b.scope()), (1, 2));
    let from_a = honoured(a.register(1, 10));
    let from_b = honoured(b.register(2, 10));
    assert_eq!((from_a.scope(), from_b.scope()), (1, 2));
    assert_eq!(from_a.slot, from_b.slot);
    assert_eq!(sequence_of(from_a.key), sequence_of(from_b.key));
    refuses(&mut b, Refusal::Stale, |b| b.cancel(from_a));
    assert_eq!(b.deadline(from_a), None);
    refuses(&mut a, Refusal::Stale, |a| a.cancel(from_b));
    assert_eq!(a.deadline(from_b), None);

    // Nor one of this scope that was never issued, or the raw zero.
    for key in [scoped(1, 2), 0] {
        let unissued = Handle {
            slot: from_a.slot,
            key,
        };
        refuses(&mut a, Refusal::Stale, |a| a.cancel(unissued));
        assert_eq!(a.deadline(unissued), None);
    }

    assert_eq!(
        (a.deadline(from_a), b.deadline(from_b)),
        (Some(10), Some(10))
    );
    assert_eq!((a.cancel(from_a), b.cancel(from_b)), (Ok(1), Ok(2)));
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
    // Scopes are the caller's: the core reads no clock and no random source,
    // so two wheels left in one scope issue the same handles and still alias.
    let mut twin = Wheel::<u64, 2>::new();
    assert_eq!(twin.register(3, 10), Ok(Handle { slot: 0, key: 1 }));
    assert_eq!(plain.cancel(Handle { slot: 0, key: 1 }), Ok(1));

    let mut scoped_wheel = Wheel::<u64, 2>::starting_at(7, 100);
    assert_eq!((scoped_wheel.scope(), scoped_wheel.now()), (7, 100));
    let first = honoured(scoped_wheel.register(1, 101));
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
fn the_last_id_of_a_scope_is_issued_and_then_the_wheel_is_exhausted() {
    let mut wheel = resumed::<2>(3, SEQUENCE_MAX - 1, 0);
    // A call refused for another reason spends no id.
    refuses(&mut wheel, Refusal::Elapsed, |w| w.register(1, 0));
    let last = honoured(wheel.register(1, 10));
    assert_eq!(
        last,
        Handle {
            slot: 0,
            key: scoped(3, SEQUENCE_MAX)
        }
    );
    assert_eq!(last.scope(), 3);
    refuses(&mut wheel, Refusal::Exhausted, |w| w.register(2, 10));
    // The last handle still works; its id is never reissued.
    assert_eq!(wheel.cancel(last), Ok(1));
    assert_eq!(wheel.register(2, 10), Err(Refusal::Exhausted));
    assert_eq!(wheel.cancel(last), Err(Refusal::Stale));
    // Resumed spent, it stays spent; one past the last sequence is no state.
    let mut spent = resumed::<2>(3, SEQUENCE_MAX, 0);
    assert_eq!((spent.scope(), spent.issued()), (3, SEQUENCE_MAX));
    refuses(&mut spent, Refusal::Exhausted, |s| s.register(1, 10));
    for issued in [SEQUENCE_MAX + 1, u64::MAX] {
        assert_eq!(Wheel::<u64, 2>::resumed(3, issued, 0), None);
    }
}

/// A fresh wheel would count from one again, and a handle that outlived the
/// restart would name the new registration filed in its entry.
#[test]
fn a_resumed_wheel_continues_its_scope_and_refuses_every_earlier_handle() {
    let mut first = Wheel::<u64, 2>::starting_at(5, 100);
    let old: [Handle; 3] = core::array::from_fn(|at| {
        let handle = honoured(first.register(at as u64, 200));
        assert_eq!(first.cancel(handle).map(|_| ()), Ok(()));
        handle
    });
    let pending = honoured(first.register(9, 300));
    assert_eq!(first.issued(), 4);

    let mut wheel = resumed::<2>(first.scope(), first.issued(), first.now());
    assert_eq!((wheel.scope(), wheel.issued(), wheel.now()), (5, 4, 100));
    // Only the sequence came back: nothing is registered.
    assert!(wheel.is_empty());
    assert_eq!(wheel.next_wake(), None);
    let next = honoured(wheel.register(7, 150));
    assert_eq!(next.key, scoped(5, 5));
    // The entry every earlier handle sat in is in use again.
    assert_eq!((next.slot, pending.slot), (old[0].slot, old[0].slot));
    let again = honoured(wheel.register(8, 150));
    assert_eq!(again.key, scoped(5, 6));

    for handle in old.into_iter().chain([pending]) {
        assert_ne!(handle.key, next.key);
        assert_ne!(handle.key, again.key);
        refuses(&mut wheel, Refusal::Stale, |w| w.cancel(handle));
        assert_eq!(wheel.deadline(handle), None);
    }
    assert_eq!((wheel.cancel(next), wheel.cancel(again)), (Ok(7), Ok(8)));
}
