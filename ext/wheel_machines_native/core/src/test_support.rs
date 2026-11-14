//! Helpers shared by the crate's test modules.

use std::vec::Vec;

use crate::{BITS, Due, LEVELS, Refusal, Wheel};

/// A wheel resumed after `issued` handle ids, to reach the end of the sequence.
pub fn resumed<const N: usize>(scope: u16, issued: u64, now: u64) -> Wheel<u64, N> {
    let Some(wheel) = Wheel::resumed(scope, issued, now) else {
        panic!("{issued} is past the last sequence of a scope");
    };
    wheel
}

/// The tests' deterministic draws.
pub use matryona_machines::testing::SplitMix64 as Rng;

/// What the tests draw besides `below`.
pub trait Draws {
    /// An index into a collection of `len` elements.
    fn pick(&mut self, len: usize) -> usize;
    /// A span whose magnitude is uniform over the wheel's eleven levels.
    fn span(&mut self) -> u64;
}

impl Draws for Rng {
    fn pick(&mut self, len: usize) -> usize {
        usize::try_from(self.below(len as u64)).unwrap_or(0)
    }

    fn span(&mut self) -> u64 {
        let level = self.below(LEVELS as u64);
        1 + self.below(1 << (BITS as u64 * level))
    }
}

/// The value of an honoured call; panics with the refusal otherwise.
pub fn honoured<T: core::fmt::Debug>(result: Result<T, Refusal>) -> T {
    match result {
        Ok(value) => value,
        Err(refusal) => panic!("expected an honoured call, got {refusal:?}"),
    }
}

/// Drain to completion with a roomy budget and return the emitted ids.
pub fn drain_ids<const N: usize>(wheel: &mut Wheel<u64, N>) -> Vec<u64> {
    let mut ids = Vec::new();
    loop {
        let mut out = [Due::new(0, 0); 4];
        let drain = wheel.drain(64, &mut out);
        ids.extend(out[..drain.emitted].iter().map(|due| due.id));
        if drain.is_caught_up() {
            return ids;
        }
    }
}
