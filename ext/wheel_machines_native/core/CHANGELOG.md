# Changelog

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
