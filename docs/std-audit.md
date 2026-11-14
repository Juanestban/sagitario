# Audit of the standard library against Rust's (S2)

S2 makes the standard library complete for a general-purpose language, with Rust's std as the
reference ([roadmap v2](roadmap-v2.md#s2-a-complete-standard-library),
[ADR-0084](adr/0084-a-complete-language-and-the-engine-apart.md), Q542). This audit compares
each module of Rust's std with what Sagitario has: what exists, what is missing, and what is out
of scope, with the reason. Each gap is filled by a record of its own; the rows say which.

The state is that of 2026-11-11, after S1: every module of `std` is written in Sagitario over the
system API (`std::sys`), and the language has F1–F6, G1 and the records up to ADR-0087.

**Legend.** ✅ done · 🟡 partial · ❌ missing · ➖ not needed: Sagitario covers it another way ·
⛔ out of scope, with its reason.

## How Sagitario differs from Rust

Some of Rust's std exists to manage ownership, which Sagitario does differently, so those rows
are ➖ rather than gaps:

- Classes are reference types counted by ARC, atomically when the program uses threads: they are
  Rust's `Box`, `Rc` and `Arc` at once. `weak` fields are `Weak`.
- A `var` field of a class can change through any reference, as Rust's `Cell` and `RefCell`
  allow; `Send` is inferred, and sharing a value between threads is capturing it.
- References (`&T`, `&mut T`, `&[T]`) are only the types of parameters, with a borrow checker but
  without lifetimes (ADR-0079): no type can hold a borrow, which rules out `Cow<'a, T>`, lock
  guards and scoped threads in their Rust form.
- There is no unwinding: a panic ends the program (exit code 101).

## Gaps found in the language

The audit of the library found gaps of the language itself, which the library needs first:

| Gap | Effect on the library | Record |
|---|---|---|
| No bitwise operators (`&`, `\|`, `^`, `!` on integers, `<<`, `>>`) | Hashing, flags, bit sets, UTF-8 and parsing are written with `*`, `/` and `%`, slower and harder to read; `BitAnd`... cannot be implemented | ✅ [ADR-0088](adr/0088-bitwise-operators-and-integer-methods.md), increment 1; the interfaces `BitAnd`... in [ADR-0101](adr/0101-the-rest-of-s2.md), increment 4 |
| `wrapping_*`, `checked_*` and `saturating_*` are promised by spec 2 ("Integer arithmetic") but not implemented | Code that must not panic on overflow in debug builds cannot say so | ✅ [ADR-0088](adr/0088-bitwise-operators-and-integer-methods.md), increment 2 |
| No `Display`: user types cannot be interpolated or turned into text (spec 2, "post-MVP") | No `to_string()`, no formatting of user types | ✅ [ADR-0089](adr/0089-display-debug-and-format-specifiers.md), increment 1 |
| No tuples (spec 2: "may be added later") | `enumerate`, `zip`, `unzip`, `partition`, `split_once` and `char_indices` cannot have Rust's signatures | ✅ [ADR-0092](adr/0092-tuples.md), increments 1 and 2; the methods returning tuples come with record 4 |
| No conversion interfaces (`From`, `Into`, `TryFrom`) nor `Default` | Conversions are named functions; `?` converts errors only to `Error` | ✅ [ADR-0094](adr/0094-conversions-and-defaults.md), increments 1 to 3 |
| `@cfg` is not allowed on fields | Classes of `std::sys` that differ by system are written twice whole | ⛔ for now: a small record, when it hurts ([ADR-0101](adr/0101-the-rest-of-s2.md), Q716) |

## Modules

### Collections (`vec`, `collections`, `slice`, `array`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `Vec<T>` | `List<T>` | ✅ | Rust's methods and its equality, order and hash ([ADR-0093](adr/0093-everyday-methods.md), increment 3): `drain` takes indices, `windows` and `chunks` give copies, `sort_unstable` is `sort` (➖); `iter().rev()` comes with the iterators of increment 5 |
| `[T; N]` | arrays | ✅ | The methods of slices, lent all their elements, those that change them on a `var` array; `==`, `<` and `Hash` (ADR-0093, increment 3) |
| `&[T]`, `&mut [T]` | slices | ✅ | Views of arrays and lists, only as parameters, with `contains`, `binary_search`, `min`, `max`, and on `&mut [T]` `sort`, `reverse`, `fill`, `swap`, `copy_from_slice` (ADR-0093, increment 3); `split_at`, `windows` and `chunks` of slices are ranges and the methods of lists (➖) |
| `HashMap<K, V>` | `Map<K, V>` | ✅ | Keeps insertion order (like `IndexMap`). `update`, `upsert` and `get_or_insert` stand for `entry`, which lends the map; `retain`, `extend`, `Map::from` of a list of pairs, `iter` and `==` ([ADR-0093](adr/0093-everyday-methods.md), increments 3 and 4) |
| `HashSet<T>` | `HashSet<T: Hash>` | ✅ | In the prelude, on `Map`: keeps insertion order; the operations between sets give iterators of copies ([ADR-0095](adr/0095-collections.md), increment 2) |
| `BTreeMap`, `BTreeSet` | `std::collections::BTreeMap`, `BTreeSet` | ✅ | B-trees ordered by `Ord`, with `range`, `range_from` and `range_to` ([ADR-0095](adr/0095-collections.md), increment 4); their iterators gather their entries when made |
| `VecDeque<T>` | `std::collections::VecDeque<T>` | ✅ | A ring buffer ([ADR-0095](adr/0095-collections.md), increment 3); `contains` waits for bounds on methods (`iter().any(...)`); `Channel` keeps its two lists |
| `BinaryHeap<T>` | `std::collections::BinaryHeap<T: Ord>` | ✅ | The largest first, and `with_compare` for any other order, which stands for `Reverse` ([ADR-0095](adr/0095-collections.md), increment 5) |
| `LinkedList<T>` | — | ⛔ | Rarely the right structure; Rust itself discourages it |

### Strings and characters (`string`, `str`, `char`, `ascii`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `String`, `&str` | `String` (immutable, ARC; `trim` and `substring` share the buffer) | ✅ | Rust's methods, with Unicode's case mappings ([ADR-0093](adr/0093-everyday-methods.md), increment 2); `split`, `split_whitespace` and `matches` give lists. `char_count` is `chars().count()` and padding is a format specifier (➖) |
| `String` (growable) | `StringBuilder` | ✅ | `insert`, `truncate`, `pop`, `capacity`, `reserve` (ADR-0093, increment 2) |
| `char` | `char` | ✅ | Unicode 17's properties and case mappings, `len_utf8`, `len_utf16`, `char::from_digit` (ADR-0093, increment 2) |
| `format!`, `fmt` | format strings `f"{expr}"` | ✅ | `Display`, the specifiers (width, alignment, precision, radix) and the debug form `{:?}`, `{:#?}` (ADR-0089); widths and precisions read from bindings (`{:>width$}`), and the debug form of an interface value, its value's ([ADR-0101](adr/0101-the-rest-of-s2.md), increment 5) |

### Numbers (`num`, `f32`, `f64`, integer modules)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| Arithmetic, casts | ✅ | ✅ | Overflow panics in debug, wraps in release |
| `f64::consts`, float methods | `std::math`, methods of numbers | ✅ | G1 |
| `checked_*`, `wrapping_*`, `saturating_*` | methods of integers | ✅ | ADR-0088; `overflowing_*` is missing |
| `MIN`, `MAX` of integers | `i32::MAX`... | ✅ | ADR-0088 |
| `count_ones`, `leading_zeros`, `rotate_left`, `swap_bytes`, `to_le_bytes` | methods of integers | ✅ | ADR-0088; `to_le_bytes`, `to_be_bytes`, `to_ne_bytes` and the `from_` functions ([ADR-0101](adr/0101-the-rest-of-s2.md), increment 1) |
| `from_str_radix`, formatting in hex | `T::from_str_radix`, and `{x:x}`, `{x:#b}` and the other radixes ([ADR-0089](adr/0089-display-debug-and-format-specifiers.md)) | ✅ | Rust's messages; a radix outside 2 to 36 panics ([ADR-0101](adr/0101-the-rest-of-s2.md), increment 1) |
| `NonZero*` | — | ⛔ | A layout optimization of `Option` that Sagitario's ARC model does not use |

### Iterators (`iter`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `Iterator`, `DoubleEndedIterator`, `Peekable` | `Iter<T>`, a class over `fn() -> Option<T>`, and `DoubleEndedIter<T>` | ✅ | Rust's adapters and consumers, `peek` on every iterator, `rev` and `next_back` on those with two ends, `collect` into a list, a map, a string or a `Result` ([ADR-0093](adr/0093-everyday-methods.md), increment 5). `scan` waits for closures that assign their captures; `enumerate`, `zip`, `take` and `skip` keep one end only until a record on lengths (`ExactSizeIterator`) |
| `IntoIterator` for user types | `Iterable<T>` | ✅ | `for` runs over the `iter()` of any type implementing it ([ADR-0095](adr/0095-collections.md), Q645) |

### Options, results and errors (`option`, `result`, `error`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `Option`, `Result` | ✅ | ✅ | Rust's methods, `Eq`, `Ord` and `Hash` ([ADR-0093](adr/0093-everyday-methods.md), increment 1). `take` and `replace` on a place ([ADR-0097](adr/0097-processes-and-environment.md), increment 1); `unwrap_or_default` ([ADR-0094](adr/0094-conversions-and-defaults.md)) |
| `std::error::Error` | the prelude's `Error` interface | ✅ | `message()` and `source()`, the chain of causes ([ADR-0101](adr/0101-the-rest-of-s2.md), increment 3); backtraces are their own row |
| `panic!`, `assert!`, `unreachable!`, `todo!` | `panic`, `assert`, `assert_eq`, `assert_ne`, `unreachable`, `todo` | ✅ | Functions that panic at the line of their call, without a message argument (ADR-0093, increment 1) |

### Comparison, hashing, operators, conversion (`cmp`, `hash`, `ops`, `convert`, `default`, `marker`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Ordering` | `Eq`, `Ord`, `Ordering` | ✅ | ADR-0081; floats have `total_compare` |
| `cmp::min`, `max`, `Reverse`, `then_with` | `min`, `max`, `clamp`, the methods of `Ordering` | ✅ | ADR-0094; `Reverse` ➖ until structs with unnamed fields: `sort_by((a, b) => b.compare(a))` |
| `Hash`, `Hasher` | `Hash`, `std::hash::Hasher`, `DefaultHasher` | ✅ | `hash()` gives a `u64`; a `Hasher`, Rust's SipHash-1-3 by default, combines the fields of a type ([ADR-0101](adr/0101-the-rest-of-s2.md), increment 3) |
| `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Neg` | ✅ | ✅ | ADR-0081 |
| `Index`, `IndexMut`, `Bit*`, `Shl`, `Shr`, `*Assign` | `Index`, `IndexMut`, `BitAnd`, `BitOr`, `BitXor`, `Not`, `Shl`, `Shr` | ✅ | ADR-0094: `index` gives the value and `index_set` writes it back, and `map[key]`; the bitwise interfaces ([ADR-0101](adr/0101-the-rest-of-s2.md), increment 4); `*Assign` is ➖, `a += b` being `a = a + b` for every type implementing the operator (Q710) |
| `Range`, `RangeInclusive` | `a..b`, `a..=b` | ✅ | Iterators with Rust's adapters, `rev` and `step_by`, and `contains` and `is_empty` written on the range ([ADR-0101](adr/0101-the-rest-of-s2.md), increment 2); a range kept in a binding is an iterator, whose length is `count()` |
| `From`, `Into`, `TryFrom`, `AsRef` | `From`, `Into`, `TryFrom`, `TryInto` | ✅ | ADR-0094: Rust's conversions of numbers, `char`, `String`, lists and maps, and `?` through `From`; `AsRef` ➖, a `&T` parameter does its work |
| `Default` | `Default`, `@default` | ✅ | ADR-0094: Rust's defaults of the prelude's types, `@default` for `#[derive(Default)]`, `unwrap_or_default`, `mem::take` and struct update |
| `Clone`, `Copy` | ➖ | ➖ | Structs copy, classes share |
| `Send`, `Sync` | `Send` | ➖ | Inferred; sharing is capturing a `Send` value |
| `Drop` | `Drop` | ✅ | For classes, run when the last strong reference goes ([ADR-0096](adr/0096-streams-files-and-drop.md), increment 1) |

### Memory and pointers (`mem`, `ptr`, `boxed`, `rc`, `cell`, `borrow`, `alloc`, `pin`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `mem::size_of`, `align_of`, `zeroed` | `std::mem` | ✅ | G1 |
| `mem::swap`, `replace`, `take` | `std::mem` | ✅ | On the place a `&mut` lends ([ADR-0094](adr/0094-conversions-and-defaults.md), Q637); `Option::take` and `replace` on a place too ([ADR-0097](adr/0097-processes-and-environment.md)) |
| `ptr::null`, `read`, `write`, `copy`, `copy_nonoverlapping` | `std::ptr` | ✅ | Also `null_mut` and `write_bytes` ([ADR-0101](adr/0101-the-rest-of-s2.md), increment 2) |
| `Box`, `Rc`, `Arc`, `Weak` | classes, `weak` | ➖ | See above |
| `Cell`, `RefCell` | `var` fields of classes | ➖ | |
| `Cow` | — | ⛔ | Needs references held in a type, which Sagitario does not have |
| `alloc`, custom allocators | — | ⛔ | The runtime's allocator; S3 decides whether to expose one |
| `Pin` | — | ⛔ | Instances of classes never move; `async` frames are classes |

### Concurrency (`sync`, `thread`, `future`, `task`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `Mutex` | `Mutex<T: Send>` | ✅ | `lock` and `try_lock` with a `MutexGuard` released by `Drop`, and `get`, `set`, `update(f)`; no poisoning, since a panic ends the program ([ADR-0098](adr/0098-locks-orderings-and-threads.md), increment 2) |
| `RwLock` | `RwLock<T: Send>` | ✅ | `read`, `write`, `try_read`, `try_write` with their guards, and `get`, `set`, `update(f)` ([ADR-0098](adr/0098-locks-orderings-and-threads.md), increment 2) |
| `Condvar` | `Condvar` | ✅ | `wait`, `wait_while`, `wait_timeout`, `wait_timeout_while`, `notify_one`, `notify_all` ([ADR-0098](adr/0098-locks-orderings-and-threads.md), increment 2) |
| `atomic::*` | `Ordering`, `fence` and twelve atomics | ✅ | Rust's orderings, `AtomicI8`/`U8`/`I16`/`U16`/`Isize`/`Ptr`, `fetch_and`/`or`/`xor`/`nand`/`max`/`min`/`update`, `compare_exchange_weak` ([ADR-0098](adr/0098-locks-orderings-and-threads.md), increment 1). `Ordering` is `sync::Ordering`, since a module of `std` is one file |
| `Once`, `OnceLock`, `LazyLock` | `static` | ➖ | A `static` is computed once, on its first use, by one thread (ADR-0083) |
| `Barrier` | `Barrier` | ✅ | ([ADR-0098](adr/0098-locks-orderings-and-threads.md), increment 2) |
| `mpsc::channel`, `sync_channel` | `Channel<T: Send>` | ✅ | `new Channel()` without a limit, `Channel::bounded(capacity)` with one (0 hands each value over), `try_send`, `try_receive` and `receive_timeout` with Rust's errors; closed by `close` rather than by dropping the last sender ([ADR-0098](adr/0098-locks-orderings-and-threads.md), increment 3) |
| `thread::spawn`, `join`, `sleep`, `available_parallelism` | ✅ | ✅ | |
| `thread::Builder` (name, stack size), `current`, `ThreadId`, `park`, `yield_now` | `Builder`, `Thread`, `ThreadId` | ✅ | `Builder::spawn` returns `io::Error` and is checked as `spawn`; `park_timeout`, `Thread::unpark`, `JoinHandle::thread`; the system gets the name ([ADR-0098](adr/0098-locks-orderings-and-threads.md), increment 4) |
| `thread::scope` | — | ⛔ for now | Scoped threads borrow the stack, which needs references held by closures |
| `thread_local!` | `@thread_local static var` | ✅ | ADR-0083 |
| `Future`, `task` | `async fn`, `std::task` | ✅ | F8; a different model from Rust's (a single-threaded executor, `spawn_blocking`) |

### Input and output (`io`, `fs`, `path`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `stdin`, `stdout`, `stderr`, `print!` | `std::io` | ✅ | Lines, whole input, exact bytes, terminals, and `Stdin`, `Stdout`, `Stderr` as readers and writers of bytes, with `Stdout::lock` ([ADR-0096](adr/0096-streams-files-and-drop.md), increment 3) |
| `Read`, `Write`, `BufRead`, `BufReader`, `BufWriter` | ✅ | ✅ | On slices, as Rust's; `BufRead` has `read_line` and `lines` for want of a lent buffer ([ADR-0096](adr/0096-streams-files-and-drop.md), increments 2 and 3) |
| `fs::read`, `write`, `read_to_string`, `create_dir(_all)`, `remove_*`, `rename`, `copy`, `read_dir` | ✅ | ✅ | |
| `File` (open, read in parts, seek, append) | `File`, `OpenOptions`, `SeekFrom` | ✅ | Closed by `Drop` or by `close` ([ADR-0096](adr/0096-streams-files-and-drop.md), increment 2) |
| `fs::metadata` (size, times, permissions), `remove_dir_all`, `canonicalize`, `symlink` | `metadata`, `symlink_metadata`, `remove_dir_all`, `canonicalize` | ✅ | Size, kind, read-only and times ([ADR-0096](adr/0096-streams-files-and-drop.md), increment 4); making links and setting permissions wait for a record of `std::os` |
| `env::set_current_dir` | `env::set_current_dir` | ✅ | [ADR-0097](adr/0097-processes-and-environment.md), increment 3 |
| `Path`, `PathBuf` | `std::path` over `String` | ✅ | `join`, `file_name`, `parent`, `extension`, `stem`, `components`, `has_root`, `is_absolute`, `with_extension`, `with_file_name`, `strip_prefix`, `starts_with`, `ends_with` ([ADR-0101](adr/0101-the-rest-of-s2.md), increment 2); `PathBuf` is ➖, a path being a `String`, and normalizing is `fs::canonicalize` |

### Networking (`net`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `IpAddr`, `Ipv4Addr`, `Ipv6Addr`, `SocketAddr`, `SocketAddrV4`, `SocketAddrV6`, `AddrParseError` | `std::net` | ✅ | Rust's text, parsing (`TryFrom<String>`, Rust's `FromStr`), order and constants ([ADR-0100](adr/0100-networking.md), increment 1) |
| `TcpListener`, `TcpStream`, `ToSocketAddrs`, `Shutdown` | `std::net` | ✅ | Blocking sockets on `Read` and `Write`, `Send`, closed by `Drop`; the compiler implements `ToSocketAddrs` for `String` and the pairs with a port; time outs, `nonblocking`, `nodelay`, `ttl`, `peek`, `try_clone`, `take_error` and `connect_timeout` ([ADR-0100](adr/0100-networking.md), increments 2 and 4). `set_linger` and `set_only_v6` are left out; TLS and HTTP are Nova packages, as in Rust |
| `UdpSocket` | `std::net::UdpSocket` | ✅ | `bind`, `send_to`, `recv_from`, `connect`, `send`, `recv`, `peer_addr`, `local_addr`, a datagram cut to the buffer on every system, and time outs, `nonblocking`, `ttl`, `broadcast`, `peek`, `peek_from`, `try_clone`, `take_error` ([ADR-0100](adr/0100-networking.md), increments 3 and 4). Multicast is a record of its own |

### Processes and environment (`process`, `env`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `Command` with `output`, `status` | `Command` | ✅ | Arguments, variables, directory, input; `Stdio`, `spawn` and `Child` with its pipes, `wait`, `try_wait`, `kill` and `wait_with_output` ([ADR-0097](adr/0097-processes-and-environment.md), increment 2). `env_remove` and `env_clear` wait (spec 11, "not yet") |
| `process::exit`, `abort`, `id` | `exit`, `abort`, `id` | ✅ | `abort` is provided by the compiler, as `exit` is ([ADR-0097](adr/0097-processes-and-environment.md), increment 3) |
| `env::args`, `var` | `args`, `variable` | ✅ | |
| `env::vars`, `set_var`, `remove_var`, `current_exe`, `temp_dir`, `home_dir` | the same, and `set_current_dir` | ✅ | `set_var` and `remove_var` are `unsafe`, as in Rust 2024 ([ADR-0097](adr/0097-processes-and-environment.md), increment 3). `current_dir` is `fs::current_dir` |
| `env::consts::OS`, `ARCH` | `env::OS`, `ARCH`, `FAMILY` | ✅ | In `std::env` itself, since a module of `std` is one file ([ADR-0097](adr/0097-processes-and-environment.md), increment 3) |

### Time (`time`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `Duration`, `Instant`, `SystemTime`, `sleep` | ✅ | ✅ | Arithmetic is named methods (`plus`, `minus`, `times`) |
| `checked_add`, `saturating_sub`, `Duration * f64`, `Display` | `Duration`'s operators, `checked_`, `saturating_`, `mul_f64` and the floats of seconds | ✅ | `Duration` has no `Display` in Rust either ([ADR-0101](adr/0101-the-rest-of-s2.md), increment 1) |
| Calendars, time zones | — | ⛔ | A Nova package, as `chrono` is for Rust |

### Other modules

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `ffi` (`CString`, `CStr`, C types, `c_void`) | `std::ffi`, `Library` | ✅ | ADR-0078; `OsString` is ⛔: paths and arguments are UTF-8 `String`s, UTF-16 on Windows inside `std::sys` |
| `os` (`unix::fs::PermissionsExt`, `windows::io::AsRawHandle`...) | `as_raw_handle` and `unsafe from_raw_handle` of `File` and the sockets | ➖ | The descriptor or `HANDLE` of each is a `usize` on every system ([ADR-0096](adr/0096-streams-files-and-drop.md), [ADR-0100](adr/0100-networking.md)); the permissions of Unix and the rest of `os` are missing; programs reach the system through `extern "C"`, without the traits Rust needs ([ADR-0101](adr/0101-the-rest-of-s2.md), Q715) |
| `hint::black_box` | `std::hint::black_box` | ✅ | ADR-0090 |
| `any`, `TypeId` | — | ⛔ | Reflection; interfaces cover the cases of a general-purpose program |
| `backtrace` | — | ⛔ until S3 | It walks the stack and reads the symbols of each system from the runtime's core, which S3 rewrites in Sagitario ([ADR-0101](adr/0101-the-rest-of-s2.md), Q714) |
| `panic::catch_unwind`, hooks | — | ⛔ | No unwinding; a panic hook (a message before exiting) could come with the backtraces |
| `primitive`, `prelude` | the prelude | ✅ | |

## Benchmarks

`benchmarks/` measures how far Sagitario is from Rust and C, and from C#, JavaScript (Node.js) and
Python, with the same programs in each language, built or run optimized. It is part of S2, and
its record settles the open questions below.

| Benchmark | What it measures |
|---|---|
| n-body, spectral norm | Numeric loops over structs and arrays |
| binary trees | Allocation and reference counting |
| word count, sorting, a map of counts | Strings and collections |
| a parallel sum, a channel pipeline | Threads, atomics and locks |
| hello, world | The start of a program: time to `main` and to its end |

- Each run records the time, the peak memory and the size of the program, per platform, in
  `benchmarks/results/`.
- A tool of the suite renders each benchmark as an animated race of the languages: an HTML page
  without dependencies, and an MP4 when `ffmpeg` is installed ([ADR-0090](adr/0090-benchmarks.md)).
- The work it should point at: retains and releases removed when a value moves or is lent,
  atomic counts only for values that cross threads, `restrict` for `&mut` parameters and slices,
  and closures that never escape kept off the heap. The goal is the speed of Rust in code without
  allocation, and close to it elsewhere.
- They run locally, with the median of five runs after a warm-up by default; each results file
  names its machine, and files of different machines are not compared (ADR-0090).
- ✅ Done: the first full run and what it points at are in [`docs/benchmarks.md`](benchmarks.md).

## Order of the records

From what most programs need to what fewer do. Each line is one record, or a few small ones:

1. **The language gaps:** bitwise operators and shifts; `wrapping_*`, `checked_*` and
   `saturating_*` (or the spec corrected); integer `MIN`/`MAX`. ✅
2. **`fmt`:** `Display` and `to_string()` for user types, a `Debug` form of any value, and
   width, precision, alignment and radix in format strings. ✅
3. **The benchmarks** (above), with `hint::black_box`, so that later work is measured. ✅
4. **The everyday methods:** those missing from `String`, `char` (Unicode), `List`, slices, `Map`,
   `Iter`, `Option` and `Result`, and `assert`, `assert_eq`, `unreachable`, `todo`. ✅
   ([ADR-0093](adr/0093-everyday-methods.md))
5. **Conversion and defaults:** `From`, `Into`, `TryFrom`, `Default`, generic `min`/`max`,
   `Index`. ✅ ([ADR-0094](adr/0094-conversions-and-defaults.md))
6. **Collections:** `HashSet`, `VecDeque`, `BTreeMap`, `BTreeSet` and `BinaryHeap`, Rust's names,
   and `collect` into them. ✅ ([ADR-0095](adr/0095-collections.md))
7. **Streams and files:** `Read`, `Write`, buffered readers and writers, `File`, metadata,
   `remove_dir_all`, bytes on stdin and stdout; and with them the question of `Drop`. ✅
   ([ADR-0096](adr/0096-streams-files-and-drop.md))
8. **Processes and environment:** `spawn` and `Child`, redirection, the rest of `env`. ✅
   ([ADR-0097](adr/0097-processes-and-environment.md))
9. **Concurrency:** `RwLock`, `Condvar`, `Barrier`, bounded channels, `try_lock`, thread names and
   ids, and atomic orderings. ✅ ([ADR-0098](adr/0098-locks-orderings-and-threads.md))
10. **Networking:** TCP and UDP sockets on the streams of 7. ✅
    ([ADR-0100](adr/0100-networking.md))

**Acceptance of S2** ([roadmap](roadmap-v2.md#s2-a-complete-standard-library)): every row above is
✅, ➖ or ⛔ with its reason, and the benchmarks run, locally, with their results recorded
([ADR-0090](adr/0090-benchmarks.md)). ✅ Met with [ADR-0101](adr/0101-the-rest-of-s2.md), the
rows the ten records left.
