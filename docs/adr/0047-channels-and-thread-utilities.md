# 0047. Channels and thread utilities

- Status: Accepted
- Date: 2026-10-25
- Refines: [ADR-0045](0045-threads-and-send.md) (Q180), [ADR-0039](0039-time-and-commands.md)
  (Q105: `sleep` also in `std::thread`)

## Context

F4 increment 3 adds message passing and the thread utilities ADR-0045 planned. Q180 settled the
outline: `new Channel<T>()` with `T: Send`, unbounded, one object with `send`, `receive` and
`close`. Increment 3 settles the details, Q194–Q201.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q194 | `send` on a closed channel | It panics (`send on a closed channel`), as in Go: closing says that nobody sends any more. |
| Q195 | `receive` | It waits until there is a value, and returns `None` once the channel is closed and empty. `channel.iter()` returns a prelude `Iter<T>` of the values received, so `for value in channel.iter()` ends when the channel is closed and empty. |
| Q196 | `close` | Any thread may close a channel, and closing it again does nothing. Values sent before closing are still received. Closing wakes every waiting receiver. |
| Q197 | Not now | Bounded channels, `try_receive` and waiting with a timeout: they can be added later without breaking programs. |
| Q198 | `thread::sleep` | `thread::sleep(d: Duration)` is the same as `time::sleep`, which stays for the programs of F3. |
| Q199 | `thread::available_parallelism()` | The number of logical processors as a `usize`, at least 1: `sysconf(_SC_NPROCESSORS_ONLN)` on Linux and Android, `sysctlbyname("hw.logicalcpu")` on macOS and iOS, `GetActiveProcessorCount` on Windows. |
| Q200 | Implementation | `Channel` is a class of `std::sync` written in Sagitario. Its queue is two lists, an inbox and an outbox, so that sending and receiving cost amortized O(1) and values are released as soon as they are received. The runtime's mutex has a condition variable (`pthread_cond_t`, `CONDITION_VARIABLE`) to wait without spinning. Like every class of `std::sync`, `Channel<T>` is `Send` when `T` is. |
| Q201 | Tests and documentation | A `run-pass` case with TSan (one producer and four consumers sharing 10000 values, `iter()`, `None` after closing, `thread::sleep`, `available_parallelism`), a `run-panic` case for sending on a closed channel, a `compile-fail` case for `Channel<List<i32>>`; this ADR, the specification and the roadmap. |

## Alternatives considered

- **`send` returning `Result<(), T>`, as in Rust:** every send would handle an error that a
  correct program never sees.
- **Separate sender and receiver objects:** clearer ownership in a language with moves; here
  both would be shared anyway.
- **A queue removing from the front of one list:** O(n) per value received.
- **Moving `sleep` out of `std::time`:** it would break programs for no gain.

## Consequences

- The runtime adds `sag_sync_mutex_wait`, `sag_sync_mutex_notify_all` and
  `sag_thread_available_parallelism`. On Apple systems it defines `_DARWIN_C_SOURCE`, since
  their headers hide `sysctlbyname` under strict POSIX; this was not built locally, and CI's
  macOS job is its first check.
- A panic in `send` reports a line of `std::sync`, not of the caller.
