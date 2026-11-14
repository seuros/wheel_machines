# wheel-machines

> **AMS Varghese**, NCC-4222, *Varghese*-class chronometer tender.

Engine-room flight manual: index deadlines in a bounded hierarchical timing
wheel and hand back the ids that are due.

Allocation-free `no_std` expiry index. A `Wheel<I, N>` holds at most `N`
registrations of caller ids `I` in eleven levels of 64 slots, which together
cover every `u64` tick: no deadline is too far away, and a full wheel refuses
new registrations instead of evicting live ones. `register` returns a
`Handle` carrying a scoped id that is never reissued; `cancel` removes a
registration that has not been emitted and returns its id. A handle that no
longer names a live registration (cancelled, drained, or its entry reused) is
refused as `Stale`, so it can never cancel the entry that reused its slot.

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
persistence and no wire format. The slot table costs 704 `usize` words
regardless of `N`.

**Scopes.** A handle is only meaningful to the wheel that issued it. Handle
ids come from a `periscope_machines::Issuer`: the wheel's scope in the top 16
bits, a sequence counted from one below. `Wheel::with_scope(scope)` and
`Wheel::starting_at(scope, now)` choose the scope; `new()` is scope zero at
tick zero. `Wheel::scope` and `Handle::scope` read it.
`cancel` answers `Stale` and `deadline` answers `None` for a handle the wheel
did not issue, before looking at any entry. Give each wheel its own scope: the
core reads no clock and no random source, so it cannot pick one, and two wheels
left in the same scope issue the same handles, so two `Wheel::new()` alias: each
cancels the other's registrations. A scope is `Exhausted` after
2^48 - 1 handles.

**Restarts.** A fresh wheel counts from one again, so a handle that outlived a
restart could name a new registration. Save `Wheel::issued` and rebuild with
`Wheel::resumed(scope, issued, now)`, or give each incarnation a new scope.
`resumed` restores only the handle sequence: the wheel is empty, every earlier
handle is `Stale`, and re-registering what was pending is the caller's job. It
is `None` when `issued` is beyond `periscope_machines::SEQUENCE_MAX`.

`Refusal` is the crate's one refusal type and a refused call changes nothing:
`register` answers `Elapsed`, `Full` or `Exhausted`, `cancel` answers `Stale`,
`advance` and `tick` answer `Regressed { now }` with the tick the wheel holds.
`drain` never refuses; its `Status` says why it stopped.

**Broken invariants.** The core never panics at run time. Every entry filed
in a slot is live; if that ever broke, `drain` would meet a filed entry that
is already free. That arm `debug_assert!`s, so tests catch it, and in release
takes the entry off its slot and carries on. It is the one arm that does not
stop with nothing changed, because it cannot: an entry left filed would be met
first on every later drain and the wheel would never emit what is due behind
it. The entry is already on the free list, so the unlink frees nothing twice
and emits nothing.

**Values.** `Drain` keeps its fields private behind `emitted()`,
`cascaded()` and `status()`; `Drain::new` builds one and answers `None` when
`emitted + cascaded` does not fit a `usize`, which no drain charges. `Due`
holds any id and deadline (`Due::new` never refuses).

**Which answers record the clock.** Only `advance` and an honoured `tick`
record `now`; re-advancing to the tick already held records the same value
and changes nothing. `register` judges `Elapsed` against the tick already held
and records none; `cancel`, `drain`, and the queries take no instant.

The Ruby gem mirrors `Handle`, `Due` and `Drain` under the same names; its
`Drain` carries the due entries where this one counts `emitted`.

`drain` and `tick` write the first `emitted` entries of `out` and leave the
rest untouched. `Due<I>` implements `Default` when `I` does, so the buffer is
`[Due::default(); N]`; the default is a placeholder the wheel never emits,
because a deadline of 0 is always refused. The wheel itself asks nothing of
`I` beyond `Copy`; for an id without `Default`, pre-fill with `Due::new`.

```rust
use wheel_machines::{Drain, Due, Refusal, Status, Wheel};

# fn main() -> Result<(), Refusal> {
let mut wheel = Wheel::<u64, 8>::new();
let lease = wheel.register(7, 5_000)?;
wheel.register(8, 90)?;
assert_eq!(wheel.register(9, 0), Err(Refusal::Elapsed));
assert_eq!(wheel.next_wake(), Some(64));

let mut out = [Due::default(); 4];
let drain = wheel.tick(100, 16, &mut out)?;
assert_eq!(Some(drain), Drain::new(1, 1, Status::CaughtUp));
assert_eq!(out[0], Due::new(8, 90));

assert_eq!(wheel.cancel(lease), Ok(7));
assert_eq!(wheel.cancel(lease), Err(Refusal::Stale));
assert_eq!(wheel.tick(99, 16, &mut out), Err(Refusal::Regressed { now: 100 }));

// Another wheel, another scope: it refuses a handle it did not issue.
let mut leases = Wheel::<u64, 8>::with_scope(1);
let mut retries = Wheel::<u64, 8>::with_scope(2);
let handle = leases.register(7, 5_000)?;
retries.register(8, 5_000)?;
assert_eq!(handle.scope(), 1);
assert_eq!(retries.cancel(handle), Err(Refusal::Stale));
assert_eq!(leases.cancel(handle), Ok(7));
# Ok(())
# }
```
