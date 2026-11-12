//! Helpers shared by the crate's test modules.

use std::vec::Vec;

use crate::{Drain, Due, Handle, Refusal, Regressed, Wheel};

/// Deterministic splitmix64 stream; the tests own their draws.
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    /// An index into a collection of `len` elements.
    pub fn pick(&mut self, len: usize) -> usize {
        usize::try_from(self.below(len as u64)).unwrap_or(0)
    }

    /// A span whose magnitude is uniform over the wheel's eleven levels.
    pub fn span(&mut self) -> u64 {
        let level = self.below(11);
        1 + self.below(1 << (6 * level))
    }
}

pub fn registered(result: Result<Handle, Refusal>) -> Handle {
    let Ok(handle) = result else {
        panic!("expected a handle, got {result:?}");
    };
    handle
}

pub fn ticked(result: Result<Drain, Regressed>) -> Drain {
    let Ok(drain) = result else {
        panic!("expected a drain, got {result:?}");
    };
    drain
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
