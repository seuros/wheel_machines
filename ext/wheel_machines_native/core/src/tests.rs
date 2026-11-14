use std::string::ToString;
use std::vec::Vec;

use periscope_machines::{SEQUENCE_MAX, scoped, sequence_of};

use super::*;
use crate::test_support::{Draws, Rng, drain_ids, honoured, resumed};
use matryona_machines::testing::refuses;

mod scope;
mod values;

const BLANK: Due<u64> = Due::new(0, 0);

#[test]
fn due_ids_come_out_in_deadline_then_registration_order() {
    let mut wheel = Wheel::<u64, 8>::new();
    for (id, deadline) in [(1, 70), (2, 5), (3, 70), (4, 4_096), (5, 5), (6, 69)] {
        honoured(wheel.register(id, deadline));
    }
    assert_eq!(wheel.len(), 6);
    assert_eq!(wheel.tick(4, 64, &mut [BLANK; 8]).map(|d| d.emitted), Ok(0));
    let mut out = [BLANK; 8];
    let drain = honoured(wheel.tick(70, 64, &mut out));
    assert_eq!(drain.status, Status::CaughtUp);
    assert_eq!(drain.emitted, 5);
    assert_eq!(
        out[..5],
        [(2, 5), (5, 5), (6, 69), (1, 70), (3, 70)].map(|(id, at)| Due::new(id, at))
    );
    assert_eq!(wheel.len(), 1);
    assert_eq!(honoured(wheel.tick(4_095, 64, &mut out)).emitted, 0);
    assert_eq!(honoured(wheel.tick(4_096, 64, &mut out)).emitted, 1);
    assert_eq!(out[0], Due::new(4, 4_096));
    assert!(wheel.is_empty());
}

#[test]
fn full_wheels_refuse_and_stale_handles_never_touch_a_reused_entry() {
    let mut wheel = Wheel::<u64, 2>::new();
    assert_eq!(wheel.capacity(), 2);
    let first = honoured(wheel.register(1, 10));
    let second = honoured(wheel.register(2, 10));
    refuses(&mut wheel, Refusal::Full, |w| w.register(3, 10));
    assert_eq!(wheel.cancel(first), Ok(1));
    refuses(&mut wheel, Refusal::Stale, |w| w.cancel(first));
    assert_eq!(wheel.deadline(first), None);
    let reused = honoured(wheel.register(3, 20));
    assert_eq!(reused.slot, first.slot);
    assert_ne!(reused, first);
    refuses(&mut wheel, Refusal::Stale, |w| w.cancel(first));
    assert_eq!(wheel.deadline(reused), Some(20));
    assert_eq!(wheel.advance(10), Ok(()));
    assert_eq!(drain_ids(&mut wheel), [2]);
    refuses(&mut wheel, Refusal::Stale, |w| w.cancel(second));
    let beyond = Handle { slot: 9, key: 1 };
    refuses(&mut wheel, Refusal::Stale, |w| w.cancel(beyond));
    assert_eq!(wheel.deadline(beyond), None);
    assert_eq!(Wheel::<u64, 0>::new().register(1, 1), Err(Refusal::Full));
}

#[test]
fn refusals_leave_the_wheel_untouched() {
    let mut wheel = Wheel::<u64, 4>::starting_at(0, 100);
    honoured(wheel.register(1, 150));
    let regressed = Refusal::Regressed { now: 100 };
    refuses(&mut wheel, regressed, |w| w.advance(99));
    let mut out = [Due::new(77, 77); 2];
    refuses(&mut wheel, regressed, |w| w.tick(0, 64, &mut out));
    assert_eq!(out, [Due::new(77, 77); 2]);
    refuses(&mut wheel, Refusal::Elapsed, |w| w.register(2, 100));
    refuses(&mut wheel, Refusal::Elapsed, |w| w.register(2, 0));
    assert_eq!(wheel.advance(100), Ok(()));
    assert_eq!(wheel.now(), 100);
    // Every refusal describes itself.
    let error: &dyn core::error::Error = &Refusal::Regressed { now: 100 };
    assert_eq!(
        error.to_string(),
        "tick is earlier than the wheel's tick 100"
    );
    assert_eq!(
        Refusal::Stale.to_string(),
        "handle does not name a live registration"
    );
}

#[test]
fn the_whole_u64_range_is_indexed() {
    let mut wheel = Wheel::<u64, 4>::new();
    honoured(wheel.register(1, u64::MAX));
    honoured(wheel.register(2, u64::MAX - 1));
    honoured(wheel.register(3, 1 << 63));
    honoured(wheel.register(4, 1));
    assert_eq!(
        honoured(wheel.tick((1 << 63) - 1, 64, &mut [BLANK; 4])).emitted,
        1
    );
    assert_eq!(wheel.next_wake(), Some(1 << 63));
    assert_eq!(wheel.advance(u64::MAX - 1), Ok(()));
    assert_eq!(drain_ids(&mut wheel), [3, 2]);
    assert_eq!(wheel.next_wake(), Some(u64::MAX));
    assert_eq!(wheel.advance(u64::MAX), Ok(()));
    assert_eq!(drain_ids(&mut wheel), [1]);
    assert_eq!(wheel.next_wake(), None);
    assert_eq!(wheel.register(5, u64::MAX), Err(Refusal::Elapsed));
    let mut late = Wheel::<u64, 1>::starting_at(0, u64::MAX - 1);
    honoured(late.register(9, u64::MAX));
    assert_eq!(late.advance(u64::MAX), Ok(()));
    assert_eq!(drain_ids(&mut late), [9]);
}

#[test]
fn cascades_and_due_entries_are_charged_against_the_budget() {
    let mut wheel = Wheel::<u64, 4>::new();
    let deadline = 64 * 64 * 64 + 64 * 64 + 64 + 1;
    honoured(wheel.register(7, deadline));
    assert_eq!(wheel.advance(deadline), Ok(()));
    let mut out = [BLANK; 4];
    let mut steps = Vec::new();
    loop {
        let drain = wheel.drain(1, &mut out);
        assert!(drain.work() <= 1);
        steps.push((drain.cascaded, drain.emitted));
        if drain.is_caught_up() {
            break;
        }
        assert_eq!(drain.status, Status::BudgetSpent);
    }
    assert_eq!(steps, [(1, 0), (1, 0), (1, 0), (0, 1)]);
    assert_eq!(out[0], Due::new(7, deadline));
    assert_eq!(wheel.drain(0, &mut out).status, Status::CaughtUp);

    let mut far = Wheel::<u64, 1>::new();
    honoured(far.register(1, u64::MAX));
    assert_eq!(far.advance(u64::MAX), Ok(()));
    let drain = far.drain(usize::MAX, &mut out);
    assert_eq!((drain.cascaded, drain.emitted), (MAX_CASCADES, 1));
    assert_eq!(far.drain(0, &mut out).status, Status::CaughtUp);
}

#[test]
fn a_full_buffer_or_spent_budget_resumes_where_it_stopped() {
    let mut wheel = Wheel::<u64, 8>::new();
    let handles: Vec<Handle> = (0..6).map(|id| honoured(wheel.register(id, 9))).collect();
    assert_eq!(wheel.advance(9), Ok(()));
    let drain = wheel.drain(64, &mut []);
    assert_eq!((drain.emitted, drain.status), (0, Status::OutputFull));
    let drain = wheel.drain(0, &mut [BLANK; 2]);
    assert_eq!((drain.emitted, drain.status), (0, Status::BudgetSpent));
    let mut out = [BLANK; 2];
    let drain = wheel.drain(64, &mut out);
    assert_eq!((drain.emitted, drain.status), (2, Status::OutputFull));
    assert_eq!(out.map(|due| due.id), [0, 1]);
    assert_eq!(wheel.next_wake(), Some(9));
    assert_eq!(wheel.cancel(handles[0]), Err(Refusal::Stale));
    assert_eq!(wheel.cancel(handles[3]), Ok(3));
    assert_eq!(wheel.advance(500), Ok(()));
    honoured(wheel.register(6, 501));
    let drain = wheel.drain(1, &mut out);
    assert_eq!((drain.emitted, drain.status), (1, Status::BudgetSpent));
    assert_eq!(out[0].id, 2);
    assert_eq!(drain_ids(&mut wheel), [4, 5]);
    assert_eq!(wheel.len(), 1);
}

#[test]
fn registering_mid_cascade_keeps_equal_deadlines_in_registration_order() {
    let mut wheel = Wheel::<u64, 8>::new();
    for id in 0..3 {
        honoured(wheel.register(id, 4_200));
    }
    honoured(wheel.register(3, 4_100));
    assert_eq!(wheel.advance(4_150), Ok(()));
    let drain = wheel.drain(1, &mut [BLANK; 8]);
    assert_eq!((drain.cascaded, drain.status), (1, Status::BudgetSpent));
    assert_eq!(wheel.next_wake(), Some(4_150));
    honoured(wheel.register(4, 4_200));
    honoured(wheel.register(5, 4_151));
    honoured(wheel.register(6, 9_000));
    assert_eq!(drain_ids(&mut wheel), [3]);
    assert_eq!(wheel.advance(4_200), Ok(()));
    assert_eq!(drain_ids(&mut wheel), [5, 0, 1, 2, 4]);
    assert_eq!(wheel.advance(9_000), Ok(()));
    assert_eq!(drain_ids(&mut wheel), [6]);
}

#[test]
fn next_wake_never_passes_the_earliest_deadline_and_always_finds_work() {
    let mut wheel = Wheel::<u64, 4>::new();
    assert_eq!(wheel.next_wake(), None);
    honoured(wheel.register(1, 300));
    let mut out = [BLANK; 4];
    let mut emitted = 0;
    while let Some(wake) = wheel.next_wake() {
        assert!(wake <= 300 && wake >= wheel.now());
        let drain = honoured(wheel.tick(wake, 64, &mut out));
        assert!(drain.work() > 0);
        emitted += drain.emitted;
    }
    assert_eq!((emitted, wheel.now()), (1, 300));
}

/// Independent naive model: an unordered list scanned and sorted on demand.
struct Model {
    now: u64,
    live: Vec<(u64, u64, u64, Handle)>,
    issued: Vec<Handle>,
    registrations: usize,
    cascades: usize,
}

impl Model {
    fn due(&self) -> Vec<(u64, u64, u64, Handle)> {
        let mut due: Vec<_> = self
            .live
            .iter()
            .copied()
            .filter(|e| e.0 <= self.now)
            .collect();
        due.sort_unstable_by_key(|e| (e.0, e.1));
        due
    }

    /// Cancel a handle issued earlier, or its twin from another wheel's scope.
    fn cancel<const N: usize>(&mut self, rng: &mut Rng, wheel: &mut Wheel<u64, N>) {
        let mut handle = self.issued[rng.pick(self.issued.len())];
        if rng.below(4) == 0 {
            // Same entry, same sequence number, another scope.
            handle.key = scoped(wheel.scope().wrapping_add(1), sequence_of(handle.key));
            assert_eq!(wheel.deadline(handle), None);
        }
        let position = self.live.iter().position(|e| e.3 == handle);
        let expected = position
            .map(|at| self.live.swap_remove(at).2)
            .ok_or(Refusal::Stale);
        let before = wheel.clone();
        assert_eq!(wheel.cancel(handle), expected);
        assert!(expected.is_ok() || *wheel == before);
    }

    fn check<const N: usize>(&self, wheel: &Wheel<u64, N>) {
        assert_eq!(wheel.len(), self.live.len());
        assert_eq!(wheel.now(), self.now);
        let earliest = self.live.iter().map(|e| e.0).min();
        match (wheel.next_wake(), earliest) {
            (None, None) => {}
            (Some(wake), Some(earliest)) => {
                assert!(wake >= self.now && wake <= earliest.max(self.now));
            }
            other => panic!("next_wake disagrees with the model: {other:?}"),
        }
    }
}

fn run_against_model<const N: usize>(seed: u64, scope: u16, start: u64, steps: usize) {
    let mut rng = Rng::new(seed);
    let mut wheel = Wheel::<u64, N>::starting_at(scope, start);
    let mut model = Model {
        now: start,
        live: Vec::new(),
        issued: Vec::new(),
        registrations: 0,
        cascades: 0,
    };
    for sequence in 0..steps as u64 {
        match rng.below(10) {
            0..=3 => {
                let deadline = match rng.below(8) {
                    0 => model.now.saturating_sub(rng.below(3)),
                    1 if !model.live.is_empty() => model.live[rng.pick(model.live.len())].0,
                    _ => model.now.saturating_add(rng.span()),
                };
                let id = rng.next_u64();
                let expected = if deadline <= model.now {
                    Err(Refusal::Elapsed)
                } else if model.live.len() == N {
                    Err(Refusal::Full)
                } else {
                    Ok(())
                };
                let before = wheel.clone();
                let outcome = wheel.register(id, deadline);
                assert_eq!(outcome.map(|_| ()), expected);
                if let Ok(handle) = outcome {
                    assert_eq!(wheel.deadline(handle), Some(deadline));
                    model.live.push((deadline, sequence, id, handle));
                    model.issued.push(handle);
                    model.registrations += 1;
                } else {
                    assert_eq!(wheel, before);
                }
            }
            4 if !model.issued.is_empty() => model.cancel(&mut rng, &mut wheel),
            5 | 6 => {
                let target = match rng.below(6) {
                    0 => model.now.saturating_sub(rng.below(3)),
                    1 | 2 if !model.live.is_empty() => {
                        let near = model.live[rng.pick(model.live.len())].0;
                        (near.saturating_add(rng.below(3)).saturating_sub(1)).max(model.now)
                    }
                    3 => model.now.saturating_add(rng.below(100)),
                    _ => model.now.saturating_add(rng.span()),
                };
                let before = wheel.clone();
                if target < model.now {
                    let refused = Err(Refusal::Regressed { now: model.now });
                    if rng.below(2) == 0 {
                        assert_eq!(wheel.advance(target), refused);
                    } else {
                        let mut out = [BLANK; 3];
                        assert_eq!(wheel.tick(target, 6, &mut out).map(|_| ()), refused);
                        assert_eq!(out, [BLANK; 3]);
                    }
                    assert_eq!(wheel, before);
                } else {
                    model.now = target;
                    assert_eq!(wheel.advance(target), Ok(()));
                }
            }
            _ => {
                let budget = rng.pick(6);
                let mut out = [BLANK; 3];
                let room = rng.pick(4);
                let drain = wheel.drain(budget, &mut out[..room]);
                let due = model.due();
                assert!(drain.work() <= budget && drain.emitted <= room.min(due.len()));
                for (got, want) in out[..drain.emitted].iter().zip(&due) {
                    assert_eq!((got.deadline, got.id), (want.0, want.2));
                    model.live.retain(|e| e.3 != want.3);
                }
                match drain.status {
                    Status::CaughtUp => assert_eq!(drain.emitted, due.len()),
                    Status::OutputFull => assert!(drain.emitted == room && room < due.len()),
                    Status::BudgetSpent => assert_eq!(drain.work(), budget),
                }
                model.cascades += drain.cascaded;
            }
        }
        model.check(&wheel);
    }
    assert!(model.cascades <= model.registrations * MAX_CASCADES);
    let drained = drain_ids(&mut wheel);
    let due: Vec<u64> = model.due().iter().map(|e| e.2).collect();
    assert_eq!(drained, due);
    assert_eq!(wheel.len(), model.live.len() - due.len());
}

#[test]
fn randomized_runs_match_the_naive_model() {
    for seed in 0..40 {
        run_against_model::<24>(seed, 0, 0, 4_000);
        run_against_model::<5>(seed ^ 0xA5A5, 0x4236, Rng::new(seed).next_u64() >> 8, 4_000);
        let late = u64::MAX - Rng::new(seed).below(1 << 20);
        run_against_model::<24>(seed ^ 0x5A5A, u16::MAX, late, 4_000);
    }
}

#[test]
fn a_default_buffer_needs_no_invented_id() {
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    enum Job {
        #[default]
        Idle,
        Renew,
        Expire,
    }
    let mut wheel = Wheel::<Job, 4>::new();
    honoured(wheel.register(Job::Expire, 9));
    honoured(wheel.register(Job::Renew, 3));
    let mut out = [Due::<Job>::default(); 4];
    assert_eq!(out[0], Due::new(Job::Idle, 0));
    assert_eq!(wheel.drain(8, &mut out).emitted, 0);
    assert_eq!(honoured(wheel.tick(9, 8, &mut out)).emitted, 2);
    assert_eq!(
        out[..2],
        [Due::new(Job::Renew, 3), Due::new(Job::Expire, 9)]
    );
    assert_eq!(out[2..], [Due::default(); 2]);
}
