# wheel-machines

> **AMS Varghese**, NCC-4222, *Varghese*-class chronometer tender.

Engine-room flight manual: index deadlines in a bounded hierarchical timing
wheel and hand back the ids that are due.

Allocation-free `no_std` expiry index. A `Wheel<I, N>` holds at most `N`
registrations of caller ids `I` in eleven levels of 64 slots, which together
cover every `u64` tick: no deadline is too far away, and a full wheel refuses
new registrations instead of evicting live ones. `register` returns a
generation-checked `Handle`; `cancel` removes a registration that has not been
emitted and returns its id. A handle that no longer names a live registration
(cancelled, drained, or its entry reused) is refused as `Stale`, so it can
never cancel the entry that reused its slot.

Time is explicit. The wheel has no clock and fires no callbacks: `advance`
moves its tick, a backwards tick is refused as `Regressed { now }`, and `drain`
writes due ids into a caller buffer in deadline order, equal deadlines in
registration order. The tick is whatever unit the caller chooses; it is a
scheduling tick, not an event-time watermark, and the wheel neither reads nor
advances one.

A tick is not O(1) and the crate does not pretend otherwise. An entry
registered far ahead sits in a coarse slot and is moved to a finer one each
time the tick enters its range, at most `MAX_CASCADES` (ten) times in its
life. `drain` takes a work budget and charges one unit per cascade move and one
per emitted id. It stops when caught up, when the buffer is full, or when the
budget is spent, and the next call resumes exactly there. Registering and
cancelling between resumed drains is allowed. `next_wake` reports the tick at
which a drain next has work, which is never later than the earliest deadline.

A deadline at or before the current tick is refused as `Elapsed` rather than
queued: the caller already knows it is due. This in-memory index has no
persistence and no wire format; handles are scoped to one wheel lifetime. The
slot table costs 704 `usize` words regardless of `N`.

`Refusal` is the crate's one refusal type and a refused call changes nothing:
`register` answers `Elapsed`, `Full` or `Exhausted`, `cancel` answers `Stale`,
`advance` and `tick` answer `Regressed { now }` with the tick the wheel holds.
`drain` never refuses; its `Status` says why it stopped.

The Ruby gem mirrors `Handle`, `Due` and `Drain` under the same names; its
`Drain` carries the due entries where this one counts `emitted`.

`drain` and `tick` write the first `emitted` entries of `out` and leave the
rest untouched. `Due<I>` implements `Default` when `I` does, so the buffer is
`[Due::default(); N]`; the default is a placeholder the wheel never emits,
because a deadline of 0 is always refused. The wheel itself asks nothing of
`I` beyond `Copy`; for an id without `Default`, pre-fill with `Due::new`.

```rust
use wheel_machines::{Due, Refusal, Status, Wheel};

let mut wheel = Wheel::<u64, 8>::new();
let lease = wheel.register(7, 5_000).expect("room for one");
let _retry = wheel.register(8, 90).expect("room for two");
assert_eq!(wheel.register(9, 0), Err(Refusal::Elapsed));
assert_eq!(wheel.next_wake(), Some(64));

let mut out = [Due::default(); 4];
let drain = wheel.tick(100, 16, &mut out).expect("time moved forward");
assert_eq!((drain.emitted, drain.cascaded, drain.status), (1, 1, Status::CaughtUp));
assert_eq!(out[0], Due::new(8, 90));

assert_eq!(wheel.cancel(lease), Ok(7));
assert_eq!(wheel.cancel(lease), Err(Refusal::Stale));
assert_eq!(wheel.tick(99, 16, &mut out), Err(Refusal::Regressed { now: 100 }));
```
