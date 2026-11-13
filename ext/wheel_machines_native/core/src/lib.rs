#![no_std]
#![doc = include_str!("../README.md")]

#[cfg(test)]
extern crate std;

use core::fmt;

use periscope_machines::{Issuer, scope_of};

/// Wheel levels; eleven six-bit digits cover every `u64` tick.
const LEVELS: usize = 11;
/// Slots per level.
const SLOTS: usize = 64;
/// Bits of the tick consumed by one level.
const BITS: usize = 6;
/// "No node" link.
const NIL: usize = usize::MAX;

/// Most cascade moves one registration can ever be charged before it is due.
pub const MAX_CASCADES: usize = LEVELS - 1;

/// Opaque registration: the entry it sits in and the scoped id it was issued
/// under. Only the wheel that issued it honours it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handle {
    slot: usize,
    /// Scoped id from the wheel's issuer; never reissued.
    key: u64,
}

impl Handle {
    /// The scope of the wheel that issued this handle.
    #[must_use]
    pub const fn scope(&self) -> u16 {
        scope_of(self.key)
    }
}

/// Why a call was refused. A refused call changes nothing: not the tick, not
/// a registration, not an issued id.
///
/// This is the crate's one refusal type. [`Wheel::register`] answers
/// [`Elapsed`](Self::Elapsed), [`Full`](Self::Full) or
/// [`Exhausted`](Self::Exhausted); [`Wheel::cancel`] answers
/// [`Stale`](Self::Stale); [`Wheel::advance`] and [`Wheel::tick`] answer
/// [`Regressed`](Self::Regressed). [`Wheel::drain`] never refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The deadline is at or before the wheel's tick: it is due now, and the
    /// caller already knows it.
    Elapsed,
    /// Every entry is in use by a registration that is neither cancelled nor drained.
    Full,
    /// Every handle id of this wheel's scope has been issued; none is reissued.
    Exhausted,
    /// The tick is earlier than the one the wheel already holds.
    Regressed {
        /// The tick the wheel holds.
        now: u64,
    },
    /// The handle does not name a live registration: it was already cancelled
    /// or drained, or this wheel never issued it (another scope, or an id not
    /// issued yet).
    Stale,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Elapsed => f.write_str("deadline is not after the wheel's tick"),
            Self::Full => f.write_str("every entry is in use"),
            Self::Exhausted => f.write_str("every handle of this scope has been issued"),
            Self::Regressed { now } => write!(f, "tick is earlier than the wheel's tick {now}"),
            Self::Stale => f.write_str("handle does not name a live registration"),
        }
    }
}

impl core::error::Error for Refusal {}

/// One due registration: the caller's id and the deadline it was registered with.
///
/// `Due::default()` (for `I: Default`) is a placeholder for pre-filling a
/// drain buffer: its deadline is 0, which is always [`Refusal::Elapsed`], so
/// the wheel never emits it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Due<I> {
    /// Caller-supplied identifier.
    pub id: I,
    /// Tick the id was registered for; `now - deadline` is its lateness.
    pub deadline: u64,
}

impl<I> Due<I> {
    /// Build a value, typically to compare against a drained one or to
    /// pre-fill a drain buffer when `I` has no `Default`.
    #[must_use]
    pub const fn new(id: I, deadline: u64) -> Self {
        Self { id, deadline }
    }
}

/// Why a drain call returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Every id due at the wheel's tick has been emitted; nothing is left to do
    /// until the tick advances.
    CaughtUp,
    /// The output buffer is full and more ids are due; drain again.
    OutputFull,
    /// The work budget is spent and work remains; drain again.
    BudgetSpent,
}

/// What one drain call did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drain {
    /// Due ids written to the front of the output buffer; one work unit each.
    pub emitted: usize,
    /// Entries moved one level down; one work unit each.
    pub cascaded: usize,
    /// Why the call returned.
    pub status: Status,
}

impl Drain {
    /// Work units charged: emitted ids plus cascade moves.
    #[must_use]
    pub const fn work(&self) -> usize {
        self.emitted + self.cascaded
    }

    /// `true` when the wheel is caught up with its tick.
    #[must_use]
    pub const fn is_caught_up(&self) -> bool {
        matches!(self.status, Status::CaughtUp)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Node<I> {
    /// `None` while the entry is on the free list.
    id: Option<I>,
    deadline: u64,
    /// Scoped id of the registration last filed here.
    key: u64,
    prev: usize,
    next: usize,
    level: usize,
    slot: usize,
}

impl<I> Node<I> {
    const FREE: Self = Self {
        id: None,
        deadline: 0,
        key: 0,
        prev: NIL,
        next: NIL,
        level: 0,
        slot: 0,
    };
}

/// Fixed-capacity hierarchical timing wheel over caller-supplied ticks.
///
/// Holds at most `N` registrations of ids `I`. Eleven levels of 64 slots index
/// every `u64` deadline, so no deadline is too far away. The wheel has no
/// clock: [`Wheel::advance`] is the only way its tick moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wheel<I, const N: usize> {
    nodes: [Node<I>; N],
    /// First node of each slot's circular list, or `NIL`.
    heads: [[usize; SLOTS]; LEVELS],
    /// One bit per non-empty slot.
    occupied: [u64; LEVELS],
    /// Slot whose entries are being moved down; finished before anything else.
    cascade: Option<(usize, usize)>,
    free: usize,
    len: usize,
    /// Tick the slots are indexed against; trails `now` while a drain is pending.
    elapsed: u64,
    now: u64,
    issuer: Issuer,
}

impl<I: Copy, const N: usize> Default for Wheel<I, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: Copy, const N: usize> Wheel<I, N> {
    /// Empty wheel at tick zero, in scope zero, with no handle issued.
    #[must_use]
    pub const fn new() -> Self {
        Self::with_scope(0)
    }

    /// Empty wheel at tick zero whose handles carry `scope`.
    ///
    /// Handles are only meaningful to the wheel that issued them. Two wheels
    /// in the same scope issue the same handles, so give every wheel whose
    /// handles could meet its own scope.
    #[must_use]
    pub const fn with_scope(scope: u16) -> Self {
        Self::starting_at(scope, 0)
    }

    /// Empty wheel whose handles carry `scope` and whose tick starts at `now`.
    #[must_use]
    pub const fn starting_at(scope: u16, now: u64) -> Self {
        Self::build(Issuer::new(scope), now)
    }

    /// Empty wheel that continues `scope` after a restart: `issued` is what
    /// [`Wheel::issued`] answered when the caller saved it, and the tick
    /// starts at `now`.
    ///
    /// Only the handle sequence is restored. Nothing is registered: every
    /// handle issued before the restart is [`Refusal::Stale`] here, and
    /// re-registering what was pending is the caller's job. The next handle
    /// carries sequence `issued + 1`, so none is reissued as long as the saved
    /// count is not lower than what the wheel had really issued; save it
    /// before handing a handle out.
    ///
    /// `None` when `issued` is beyond `periscope_machines::SEQUENCE_MAX`,
    /// which no wheel reaches.
    #[must_use]
    pub const fn resumed(scope: u16, issued: u64, now: u64) -> Option<Self> {
        match Issuer::resumed(scope, issued) {
            Some(sequence) => Some(Self::build(sequence, now)),
            None => None,
        }
    }

    const fn build(issuer: Issuer, now: u64) -> Self {
        let mut nodes = [Node::<I>::FREE; N];
        let mut index = 0;
        while index < N {
            nodes[index].next = if index + 1 < N { index + 1 } else { NIL };
            index += 1;
        }
        Self {
            nodes,
            heads: [[NIL; SLOTS]; LEVELS],
            occupied: [0; LEVELS],
            cascade: None,
            free: if N == 0 { NIL } else { 0 },
            len: 0,
            elapsed: now,
            now,
            issuer,
        }
    }

    /// The scope carried by every handle this wheel issues.
    #[must_use]
    pub const fn scope(&self) -> u16 {
        self.issuer.scope()
    }

    /// Handles issued in this scope so far, live or not: the count to save
    /// for [`Wheel::resumed`].
    #[must_use]
    pub const fn issued(&self) -> u64 {
        self.issuer.issued()
    }

    /// Compiled capacity `N`.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Registrations neither cancelled nor drained, due ones included.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// `true` when nothing is registered.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The latest tick accepted by [`Wheel::advance`].
    #[must_use]
    pub const fn now(&self) -> u64 {
        self.now
    }

    /// Deadline of a live registration; `None` for a handle that names none,
    /// one this wheel never issued included.
    #[must_use]
    pub fn deadline(&self, handle: Handle) -> Option<u64> {
        self.lookup(handle).map(|index| self.nodes[index].deadline)
    }

    /// Tick at which a drain next has work: never later than the earliest
    /// registered deadline, and equal to [`Wheel::now`] when work is already
    /// pending. `None` when the wheel is empty.
    ///
    /// It can be earlier than the earliest deadline: a far entry is first
    /// visited to be cascaded, not emitted.
    #[must_use]
    pub fn next_wake(&self) -> Option<u64> {
        let (level, slot) = self.next_slot()?;
        let start = match self.cascade {
            Some((level, slot)) if self.heads[level][slot] != NIL => self.elapsed,
            Some(_) | None => slot_start(self.elapsed, level, slot),
        };
        Some(start.max(self.now))
    }

    /// Index `id` to become due at tick `deadline`, which must be after
    /// [`Wheel::now`]. A refusal changes nothing.
    ///
    /// # Errors
    ///
    /// [`Refusal::Elapsed`] for a deadline at or before the tick,
    /// [`Refusal::Full`] when all `N` entries are in use, and
    /// [`Refusal::Exhausted`] once all `periscope_machines::SEQUENCE_MAX`
    /// (2^48 - 1) handle ids of the wheel's scope have been issued.
    pub const fn register(&mut self, id: I, deadline: u64) -> Result<Handle, Refusal> {
        if deadline <= self.now {
            return Err(Refusal::Elapsed);
        }
        if self.free == NIL {
            return Err(Refusal::Full);
        }
        // Last check: an id is only spent on a registration that is honoured.
        let key = match self.issuer.issue() {
            Ok(key) => key,
            Err(periscope_machines::Refusal::Exhausted) => return Err(Refusal::Exhausted),
        };
        let index = self.free;
        self.free = self.nodes[index].next;
        self.len += 1;
        let node = &mut self.nodes[index];
        node.id = Some(id);
        node.deadline = deadline;
        node.key = key;
        // A deadline inside the range being cascaded queues behind the entries
        // still waiting there, so equal deadlines keep registration order.
        match self.cascade {
            Some((level, slot)) if level_of(self.elapsed, deadline) < level => {
                self.push_back(level, slot, index);
            }
            Some(_) | None => self.settle(index),
        }
        Ok(Handle { slot: index, key })
    }

    /// Remove a registration that has not been emitted, returning its id.
    ///
    /// # Errors
    ///
    /// [`Refusal::Stale`] for a handle that does not name a live registration:
    /// already cancelled, already drained, or never issued by this wheel
    /// (another scope). The wheel is unchanged. A repeat on a dead handle is
    /// never honoured: the wheel keeps no record of how a registration ended.
    pub fn cancel(&mut self, handle: Handle) -> Result<I, Refusal> {
        let index = self.lookup(handle).ok_or(Refusal::Stale)?;
        let due = self.release(index).ok_or(Refusal::Stale)?;
        Ok(due.id)
    }

    /// Move the wheel's tick forward. Emits nothing; call [`Wheel::drain`].
    ///
    /// # Errors
    ///
    /// [`Refusal::Regressed`] when `now` is earlier than the tick already
    /// held; the wheel is unchanged. Repeating the current tick is accepted.
    pub const fn advance(&mut self, now: u64) -> Result<(), Refusal> {
        if now < self.now {
            return Err(Refusal::Regressed { now: self.now });
        }
        self.now = now;
        Ok(())
    }

    /// Emit ids due at [`Wheel::now`] into the front of `out`, doing at most
    /// `budget` work units. Each emitted id and each cascade move costs one.
    ///
    /// Ids come out in deadline order, equal deadlines in registration order.
    /// The call stops when caught up, when `out` is full, or when the budget is
    /// spent; call again to resume exactly where it stopped. It never moves the
    /// tick.
    pub fn drain(&mut self, budget: usize, out: &mut [Due<I>]) -> Drain {
        let mut drain = Drain {
            emitted: 0,
            cascaded: 0,
            status: Status::CaughtUp,
        };
        loop {
            if let Some((level, slot)) = self.cascade {
                loop {
                    let head = self.heads[level][slot];
                    if head == NIL {
                        break;
                    }
                    if drain.work() == budget {
                        drain.status = Status::BudgetSpent;
                        return drain;
                    }
                    self.unlink(head);
                    self.settle(head);
                    drain.cascaded += 1;
                }
                self.cascade = None;
            }
            let next = self
                .next_slot()
                .map(|(level, slot)| (level, slot, slot_start(self.elapsed, level, slot)));
            let Some((level, slot, start)) = next.filter(|next| next.2 <= self.now) else {
                self.elapsed = self.now;
                return drain;
            };
            self.elapsed = start;
            if level > 0 {
                self.cascade = Some((level, slot));
                continue;
            }
            loop {
                let head = self.heads[0][slot];
                if head == NIL {
                    break;
                }
                if drain.work() == budget {
                    drain.status = Status::BudgetSpent;
                    return drain;
                }
                let Some(place) = out.get_mut(drain.emitted) else {
                    drain.status = Status::OutputFull;
                    return drain;
                };
                if let Some(due) = self.release(head) {
                    *place = due;
                    drain.emitted += 1;
                }
            }
        }
    }

    /// [`Wheel::advance`] then [`Wheel::drain`] in one call.
    ///
    /// # Errors
    ///
    /// [`Refusal::Regressed`] when `now` is earlier than the tick already
    /// held; nothing is drained, `out` is not written and the wheel is
    /// unchanged.
    pub fn tick(&mut self, now: u64, budget: usize, out: &mut [Due<I>]) -> Result<Drain, Refusal> {
        self.advance(now)?;
        Ok(self.drain(budget, out))
    }

    /// Origin first: a handle this wheel's issuer never handed out names
    /// nothing here, whatever entry it points at.
    fn lookup(&self, handle: Handle) -> Option<usize> {
        if !self.issuer.owns(handle.key) {
            return None;
        }
        let node = self.nodes.get(handle.slot)?;
        (node.id.is_some() && node.key == handle.key).then_some(handle.slot)
    }

    /// Earliest non-empty slot: lower levels always expire before higher ones,
    /// and within a level the lowest set bit is the soonest.
    fn next_slot(&self) -> Option<(usize, usize)> {
        self.occupied
            .iter()
            .enumerate()
            .find(|(_, bits)| **bits != 0)
            .map(|(level, bits)| (level, bits.trailing_zeros() as usize))
    }

    /// File a node under the slot its deadline has relative to `elapsed`.
    const fn settle(&mut self, index: usize) {
        let deadline = self.nodes[index].deadline;
        let level = level_of(self.elapsed, deadline);
        self.push_back(level, digit(deadline, level), index);
    }

    const fn push_back(&mut self, level: usize, slot: usize, index: usize) {
        let head = self.heads[level][slot];
        if head == NIL {
            self.heads[level][slot] = index;
            self.occupied[level] |= 1 << slot;
            self.nodes[index].prev = index;
            self.nodes[index].next = index;
        } else {
            let tail = self.nodes[head].prev;
            self.nodes[index].prev = tail;
            self.nodes[index].next = head;
            self.nodes[tail].next = index;
            self.nodes[head].prev = index;
        }
        self.nodes[index].level = level;
        self.nodes[index].slot = slot;
    }

    const fn unlink(&mut self, index: usize) {
        let Node {
            prev,
            next,
            level,
            slot,
            ..
        } = self.nodes[index];
        if next == index {
            self.heads[level][slot] = NIL;
            self.occupied[level] &= !(1 << slot);
        } else {
            self.nodes[prev].next = next;
            self.nodes[next].prev = prev;
            if self.heads[level][slot] == index {
                self.heads[level][slot] = next;
            }
        }
    }

    /// Unlink a live node and return it to the free list.
    fn release(&mut self, index: usize) -> Option<Due<I>> {
        let id = self.nodes[index].id?;
        self.unlink(index);
        let node = &mut self.nodes[index];
        node.id = None;
        node.next = self.free;
        self.free = index;
        self.len -= 1;
        Some(Due {
            id,
            deadline: node.deadline,
        })
    }
}

/// Level of the highest six-bit digit in which `deadline` differs from `elapsed`.
const fn level_of(elapsed: u64, deadline: u64) -> usize {
    let masked = (elapsed ^ deadline) | 63;
    (63 - masked.leading_zeros() as usize) / BITS
}

/// The six-bit digit of `tick` at `level`.
const fn digit(tick: u64, level: usize) -> usize {
    ((tick >> (BITS * level)) & 63) as usize
}

/// First tick covered by `slot` of `level` in the rotation `elapsed` is in.
const fn slot_start(elapsed: u64, level: usize, slot: usize) -> u64 {
    let shift = BITS * (level + 1);
    let high = if shift >= 64 {
        0
    } else {
        (elapsed >> shift) << shift
    };
    high | ((slot as u64) << (BITS * level))
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
