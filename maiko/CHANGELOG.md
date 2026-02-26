# [0.3.0](https://github.com/maiko-rs/maiko/compare/v0.2.6...v0.3.0) (unreleased)

**Breaking changes** - API cleanup guided by the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/).

### Added

- `Context::stop_runtime()` to shut down the entire runtime from within an actor ([#92])
- Internal broadcast command channel for unified control flow ([#92])
- `# Errors` documentation on all public `Result`-returning methods
- `EventId` newtype struct (was `type EventId = u128`)
- `Error::external()` helper for wrapping errors into `Error::External`
- `ActorId`: `From<&str>`, `From<String>`, `AsRef<str>` impls
- `Debug` impl for `Supervisor` (shows actor names and task count)
- `#[must_use]` on `ActorBuilder` to catch forgotten `.build()` calls
- `Drop` impl for `MonitorRegistry` for safety-net cleanup

### Changed

- **Breaking:** `Config` renamed to `SupervisorConfig`
- **Breaking:** `Actor::on_error()` takes `&mut self` instead of `&self`
- **Breaking:** `Error::IoError` now wraps `Arc<std::io::Error>` with `#[source]` (was `String`)
- **Breaking:** `ActorId::new()` takes `&str` instead of `Arc<str>`
- **Breaking:** `ActorId::name()` renamed to `as_str()`
- **Breaking:** `Envelope` and `ActorId` no longer implement `Deref`
- **Breaking:** `Context::send()` accepts `Into<IntoEnvelope<E>>` instead of `Into<E>`
- **Breaking:** `Context::send_child_event()` takes `EventId` instead of `&Meta`
- **Breaking:** `Meta::parent()` renamed to `parent_id()`
- **Breaking:** `Envelope::with_parent()` renamed to `with_parent_id()`
- **Breaking:** `correlation_id` renamed to `parent_id` throughout
- **Breaking:** `Error::IOError` renamed to `IoError`; `Error::SendError` renamed to `MailboxClosed`; `Error::ChannelIsFull` renamed to `MailboxFull`
- **Breaking:** `Error::ActorJoinError` removed, replaced by `Error::Internal`
- **Breaking:** `Context::send_envelope()` made private
- **Breaking:** `Context::stop()` now stops only the calling actor (was system-wide); use `stop_runtime()` for full shutdown
- **Breaking:** `Supervisor::run()`, `join()`, `stop()` now consume `self`, preventing use-after-shutdown ([#43])
- Improved `Debug` impls across the codebase
- Improved documentation with examples and doc comments on previously undocumented methods
- Monitoring system uses `MonitorCommand::Shutdown` instead of `CancellationToken`
- Graceful shutdown collects all task errors instead of short-circuiting on first failure

### Removed

- **Breaking:** `Context::pending()` removed (use `StepAction::Never` instead)
- **Breaking:** `Context::send_with_correlation()` (use `send_child_event()` instead)
- **Breaking:** `Deref` impls on `Envelope` (use `event()`) and `ActorId` (use `as_str()`)
- **Breaking:** `Context::is_alive()` removed (internal to actor controller)
- **Breaking:** `Config::maintenance_interval` removed (broker cleanup is now reactive via commands)
- **Breaking:** Deprecated methods removed from `Config`
- `tokio-util` dependency removed (was used for `CancellationToken` in monitoring)

[#43]: https://github.com/maiko-rs/maiko/issues/43
[#92]: https://github.com/maiko-rs/maiko/issues/92

---

# [0.2.6](https://github.com/maiko-rs/maiko/compare/v0.2.5...v0.2.6) (February 22nd, 2026)

### Added

- `ActorMonitor` to track monitor lifecycle ([#83], [#86])
- `Why Maiko` and `Getting started` docs

### Changed

- actor-broker communication moved from single global channel to channel per actor-broker pair ([#84])
- moved repo from [personal account](https://github.com/ddrcode) to [maiko-rs](https://github.com/maiko-rs) organization ([#87])
- documentation improvements

[#83]: https://github.com/maiko-rs/maiko/pull/83
[#84]: https://github.com/maiko-rs/maiko/pull/84
[#86]: https://github.com/maiko-rs/maiko/pull/86
[#87]: https://github.com/maiko-rs/maiko/pull/87

---

# [0.2.5](https://github.com/maiko-rs/maiko/compare/v0.2.4...v0.2.5) (February 17th, 2026)

**Backpressure handling**

### Added

- `OverflowPolicy` enum ([#75])
- `overflow_policy` method in `Topic` (defaulting to `OverflowPolicy::Fail`) ([#75])
- `on_overflow` method in `Monitor` (and implementation in `Tracer`) ([#75])
- `ActorConfig` - configs allowing to fine-tune actors behavior ([#76])
- `ActorBuilder` - for more fine-grained actor building (including config) ([#76])

### Changed

- Moved from MIT to dual license (MIT + Apache 2.0) ([#77])
- Event broker to respect `OverflowPolicy` ([#75])
- The main `Config` to act as a default for `ActorConfig` ([#76])
- Non-breaking changes to align Maiko with the API guidelines ([#70])

[#70]: https://github.com/maiko-rs/maiko/issues/70
[#75]: https://github.com/maiko-rs/maiko/pull/75
[#76]: https://github.com/maiko-rs/maiko/pull/76
[#77]: https://github.com/maiko-rs/maiko/pull/77

---

# [0.2.4](https://github.com/maiko-rs/maiko/compare/v0.2.3...v0.2.4) (February 15th, 2026)

### Bug fixes

- Fixes dependency on old version of maiko-macros (#71)

[#71]: https://github.com/maiko-rs/maiko/issues/71

---

# [0.2.3](https://github.com/maiko-rs/maiko/compare/v0.2.2...v0.2.3) (February 12th, 2026)

**Contains Breaking changes** (in test harness only)

### Added

- `Label` trait and derive macro ([#56])
- `to_mermaid` method in `Supervisor` ([#56])
- `to_json` method in `Supervisor` ([#59])
- `EventChain` in test harness for correlation tracking ([#62])
- `ActorTrace` and `EventTrace` views on `EventChain` with `exact()`, `segment()`, and `passes_through()` methods ([#62])
- `EventQuery` - multiple new methods ([#66])
- `EventSpy.not_delivered_to()`, `.was_delivered_to_all()`, `.delivery_ratio()`
- `Harness.settle_on()` — condition-based settling with `Expectation` builder and `.within()` timeout ([#66])
- `Error::SettleTimeout` variant (cfg-gated behind `test-harness`)

### Changed

- Broker logic to drop events rather than fail on overflow ([#67])
- **Breaking:** `EventRecords` now uses `Arc<Vec<EventEntry>>` internally (zero-copy sharing across spies/queries) ([#62])
- **Breaking:** `ActorSpy` and `EventSpy` - polished API with some methods renamed ([#62], [#66])
- **Breaking:** `EventQuery.collect()` now returns unique events (deduplicated by ID); old behavior available via `all_deliveries()` ([#62])

### Removed

- **Breaking:** `Harness.start_recording()`, `stop_recording()`, `settle_with_timeout()` removed (use `record()`, `settle()`, `settle_on()` instead) ([#66])

[#56]: https://github.com/maiko-rs/maiko/pull/56
[#59]: https://github.com/maiko-rs/maiko/pull/59
[#62]: https://github.com/maiko-rs/maiko/pull/62
[#66]: https://github.com/maiko-rs/maiko/pull/66
[#67]: https://github.com/maiko-rs/maiko/pull/67

---

# [0.2.2](https://github.com/maiko-rs/maiko/compare/v0.2.1...v0.2.2) (February 4th, 2026)

### Added

- `Recorder` monitor for recording events to JSON Lines files ([#49])
- `Tracer` monitor for logging event lifecycle via `tracing` crate ([#52])
- `monitors` module for built-in monitor implementations ([#52])

### Changed

- `Error` enum no longer implements `Clone` (allows deriving from non-Clone errors) ([#49])
- Reorganized monitoring: `monitoring` module for engine, `monitors` for implementations ([#52])
- Improved documentation

[#49]: https://github.com/maiko-rs/maiko/pull/49
[#52]: https://github.com/maiko-rs/maiko/pull/52

---

# [0.2.1](https://github.com/maiko-rs/maiko/compare/v0.2.0...v0.2.1) (January 29th, 2026)

### Added

- [Frequently Asked Questions](https://github.com/maiko-rs/maiko/blob/main/docs/faq.md)

### Changed

- `ActorId` has a public constructor now
- Improved documentation and README
- Moved `doc` to `docs`

---

# [0.2.0](https://github.com/maiko-rs/maiko/compare/v0.1.1...v0.2.0) (January 27th, 2026)

**Contains Breaking changes**

### Key new Features

1. Adopted Maiko to work with project [Charon](https://github.com/ddrcode/charon)
as a first library use case. [#23]
2. Number of ergonomy improvements (API changes!) [#33], [#37]
3. Monitoring API [#36]
4. Test harness [#31]

### Added

- `StepAction` enum to control `step` function behavior
- `serde` feature that makes events serializable/deserializable
- `External` option in `Error` enum
- `clone_name` method in `Context`

### Changed

- Renamed `tick` to `step` in `Actor`
- Renamed `handle` to `handle_event` in `Actor`
- Fields of `Envelope` made private (use methods instead)

[#23]: https://github.com/maiko-rs/maiko/pull/23
[#31]: https://github.com/maiko-rs/maiko/pull/31
[#33]: https://github.com/maiko-rs/maiko/pull/33
[#36]: https://github.com/maiko-rs/maiko/pull/36
[#37]: https://github.com/maiko-rs/maiko/pull/37


---

# [0.1.1](https://github.com/maiko-rs/maiko/compare/v0.1.0...v0.1.1) (December 18th, 2025)

### Added

- `hello-wrold.rs` example
- `maintenance_interval` option added to `Config`
- `Broker` removes closed subscribers in periodical `cleanup` method
- `pending` method added to `Context`
- graceful shutdown - completes pending events before stop

### Changed

- Main `ActorHandle` loop to work with `tokio::select!`
- Detailed documentation aded to examples.
- `PingPong` example with events as topics.
- Improved performance of subscribers lookup in `Broker`
- `Broadcast` topic renamed to `DefaultTopic`
- `Broker` has now dedicated cancellation token (rather than shared one with actors)


---

# [0.1.0](https://github.com/maiko-rs/maiko/compare/v0.0.2...v0.1.0) (December 14th, 2025)

**MVP**. Fully-functional, tested, yet quite minimal version.

### Added

- `Event` derive macro ([#3], [#9])
- Documentation and examples ([#4])
- Event correlation logic ([#7])

### Removed

- dependency on `async-trait` ([#5])

### Changed

- renamed `DefaultTopic` to `Broadcast` ([#11])
- changed channel data type from `Envelope<E>` to `Arc<Envelope<E>>` ([#11])
- made broker working with non-blocking send

[#3]: https://github.com/maiko-rs/maiko/issues/3
[#4]: https://github.com/maiko-rs/maiko/issues/4
[#5]: https://github.com/maiko-rs/maiko/issues/5
[#7]: https://github.com/maiko-rs/maiko/issues/7
[#9]: https://github.com/maiko-rs/maiko/pull/9
[#11]: https://github.com/maiko-rs/maiko/pull/11

---

# 0.0.2 (December 9th, 2025)

First working version
