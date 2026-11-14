# Changelog

## [0.5.1](https://github.com/seuros/wheel_machines/compare/wheel_machines/v0.5.0...wheel_machines/v0.5.1) (2026-11-14)


### Bug Fixes

* **deps:** require matryona-machines &gt;= 0.3 with a names_only refusal table ([5a85b47](https://github.com/seuros/wheel_machines/commit/5a85b472af8714f8d6a08ff08295032c5fd85c56))

## [0.5.0](https://github.com/seuros/wheel_machines/compare/wheel_machines/v0.4.0...wheel_machines/v0.5.0) (2026-11-14)


### ⚠ BREAKING CHANGES

* adopt matryona-machines 0.2: Status symbols! table, exact-class Handle, exact Due fields, keep test support out of the crate

### Features

* adopt matryona-machines 0.2: Status symbols! table, exact-class Handle, exact Due fields, keep test support out of the crate ([a7c8651](https://github.com/seuros/wheel_machines/commit/a7c8651e40b8573af6853ef2a4bef45c8027eeb2))

## [0.4.0](https://github.com/seuros/wheel_machines/compare/wheel_machines/v0.3.0...wheel_machines/v0.4.0) (2026-11-14)


### ⚠ BREAKING CHANGES

* seal values, check exact classes natively, drain and tick never lose an entry

### Features

* seal values, check exact classes natively, drain and tick never lose an entry ([92a1f59](https://github.com/seuros/wheel_machines/commit/92a1f593f23777cd0db0a24e6000aa06c0a66141))

## [0.3.0](https://github.com/seuros/wheel_machines/compare/wheel_machines/v0.2.0...wheel_machines/v0.3.0) (2026-11-13)


### ⚠ BREAKING CHANGES

* handles are issued through periscope-machines. Every instance can be built with a scope; a handle from another scope is refused before any lookup; all handle kinds of an instance share one sequence; resumed restores the sequence after a restart. A handle from another wheel answers Stale. with_scope_starting_at is gone.

### Features

* scope handles through periscope-machines, add Wheel::resumed and issued, starting_at takes the scope ([3d4b484](https://github.com/seuros/wheel_machines/commit/3d4b48460f280fc78ef504384942f38527992282))

## [0.2.0](https://github.com/seuros/wheel_machines/compare/wheel_machines/v0.1.2...wheel_machines/v0.2.0) (2026-11-13)


### ⚠ BREAKING CHANGES

* see CONVENTIONS: one exhaustive Refusal per crate, Result from every mutator, honoured idempotent repeats, Ruby mirrors the core. No compatibility aliases. cancel returns Result<I, Refusal>; the Ruby Timer class is Handle.

### Features

* fold Regressed into Refusal, refuse stale cancels, Ruby Handle ([4fa0048](https://github.com/seuros/wheel_machines/commit/4fa00481e46931a9563d0ac024adeb4e5dd954a8))

## [0.1.2](https://github.com/seuros/wheel_machines/compare/wheel_machines/v0.1.1...wheel_machines/v0.1.2) (2026-11-13)


### Features

* implement Default for Due so drain buffers need no dummy id ([1dcc6aa](https://github.com/seuros/wheel_machines/commit/1dcc6aae2e0ac403e302ca0fac688d32497eac7f))

## [0.1.1](https://github.com/seuros/wheel_machines/compare/wheel_machines-v0.1.0...wheel_machines/v0.1.1) (2026-11-12)


### Features

* initial extraction ([5f1d698](https://github.com/seuros/wheel_machines/commit/5f1d698edae4429ef7e51ca07461b26197e77eb1))
