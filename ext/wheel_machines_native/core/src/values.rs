//! What a drain hands back: the due entries and what the call did.

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
///
/// Its work, emitted ids plus cascade moves, always fits a `usize`: the
/// wheel never charges more than the budget it was given, and
/// [`Drain::new`] refuses a pair whose sum would not fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drain {
    pub(crate) emitted: usize,
    pub(crate) cascaded: usize,
    pub(crate) status: Status,
}

impl Drain {
    /// Build a value, typically to compare against a drained one. `None`
    /// when `emitted + cascaded` does not fit a `usize`: no drain charges
    /// that much work.
    #[must_use]
    pub const fn new(emitted: usize, cascaded: usize, status: Status) -> Option<Self> {
        if emitted.checked_add(cascaded).is_none() {
            return None;
        }
        Some(Self {
            emitted,
            cascaded,
            status,
        })
    }

    /// Due ids written to the front of the output buffer; one work unit each.
    #[must_use]
    pub const fn emitted(&self) -> usize {
        self.emitted
    }

    /// Entries moved one level down; one work unit each.
    #[must_use]
    pub const fn cascaded(&self) -> usize {
        self.cascaded
    }

    /// Why the call returned.
    #[must_use]
    pub const fn status(&self) -> Status {
        self.status
    }

    /// Work units charged: emitted ids plus cascade moves.
    #[must_use]
    pub const fn work(&self) -> usize {
        self.emitted + self.cascaded
    }

    /// Whether one more work unit fits `budget`; marks the call
    /// [`Status::BudgetSpent`] when it does not.
    pub(crate) const fn affords(&mut self, budget: usize) -> bool {
        if self.work() == budget {
            self.status = Status::BudgetSpent;
            return false;
        }
        true
    }

    /// `true` when the wheel is caught up with its tick.
    #[must_use]
    pub const fn is_caught_up(&self) -> bool {
        matches!(self.status, Status::CaughtUp)
    }
}
