# Changelog

## [0.6.0](https://github.com/seuros/wheel_machines/compare/wheel-machines-v0.5.0...wheel-machines-v0.6.0) (2026-11-14)


### ⚠ BREAKING CHANGES

* seal values, check exact classes natively, drain and tick never lose an entry

### Features

* seal values, check exact classes natively, drain and tick never lose an entry ([92a1f59](https://github.com/seuros/wheel_machines/commit/92a1f593f23777cd0db0a24e6000aa06c0a66141))

## [0.5.0](https://github.com/seuros/wheel_machines/compare/wheel-machines-v0.4.0...wheel-machines-v0.5.0) (2026-11-13)


### ⚠ BREAKING CHANGES

* handles are issued through periscope-machines. Every instance can be built with a scope; a handle from another scope is refused before any lookup; all handle kinds of an instance share one sequence; resumed restores the sequence after a restart. A handle from another wheel answers Stale. with_scope_starting_at is gone.

### Features

* scope handles through periscope-machines, add Wheel::resumed and issued, starting_at takes the scope ([3d4b484](https://github.com/seuros/wheel_machines/commit/3d4b48460f280fc78ef504384942f38527992282))

## [0.4.0](https://github.com/seuros/wheel_machines/compare/wheel-machines-v0.3.0...wheel-machines-v0.4.0) (2026-11-13)


### ⚠ BREAKING CHANGES

* see CONVENTIONS: one exhaustive Refusal per crate, Result from every mutator, honoured idempotent repeats, Ruby mirrors the core. No compatibility aliases. cancel returns Result<I, Refusal>; the Ruby Timer class is Handle.

### Features

* fold Regressed into Refusal, refuse stale cancels, Ruby Handle ([4fa0048](https://github.com/seuros/wheel_machines/commit/4fa00481e46931a9563d0ac024adeb4e5dd954a8))

## [0.3.0](https://github.com/seuros/wheel_machines/compare/wheel-machines-v0.2.0...wheel-machines-v0.3.0) (2026-11-13)


### Features

* implement Default for Due so drain buffers need no dummy id ([1dcc6aa](https://github.com/seuros/wheel_machines/commit/1dcc6aae2e0ac403e302ca0fac688d32497eac7f))

## [0.2.0](https://github.com/seuros/wheel_machines/compare/wheel-machines-v0.1.0...wheel-machines-v0.2.0) (2026-11-12)


### Features

* initial extraction ([5f1d698](https://github.com/seuros/wheel_machines/commit/5f1d698edae4429ef7e51ca07461b26197e77eb1))
