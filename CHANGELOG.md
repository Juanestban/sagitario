# Changelog

All notable changes to Sagitario: the language, the compiler `sag`, the package manager Nova,
the language server and the VS Code extension, which share one version. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/) (before 1.0, a minor version may break programs).

Released versions are the tags `vX.Y.Z`, published as GitHub Releases. Versions written in the
sources but never tagged (0.11.0, 0.18.0, 0.19.0, 0.21.0, 0.23.0) are part of the release that
followed them.

## [Unreleased]

The sources say 0.32.0.

### Changed

- **Breaking:** `include_str("path")` is now the macro `include_str!("path")`, as in Rust, valid in
  any expression and not only in constants; the old form is an error that gives the new one
  ([ADR-0104](docs/adr/0104-include-str-as-a-macro.md)). The seed keeps the units of the FFI of
  its C apart, `bootstrap/sag_ffi.c` and `bootstrap/sag_std_ffi.c`, which `--update-seed` writes
  and the bootstrap compiles on their own: since the standard library exports functions to the
  runtime, one file of C no longer compiled.

- **Breaking:** `io::ErrorKind` gains Rust's `ConnectionRefused`, `ConnectionReset`,
  `ConnectionAborted`, `NotConnected`, `AddrInUse`, `AddrNotAvailable`, `BrokenPipe`,
  `WouldBlock`, `TimedOut` and `Interrupted`, in Rust's order; a `match` on it without `_` must
  add them. A write to a pipe whose other end closed is now `BrokenPipe` rather than `Other`
  ([ADR-0100](docs/adr/0100-networking.md), Q697).

- Every operation of an atomic takes its ordering, as Rust's: `load(Ordering::SeqCst)`,
  `fetch_add(1, Ordering::Relaxed)`, `compare_exchange(current, desired, success, failure)`, with
  `std::sync::Ordering`; add `Ordering::SeqCst` to keep what a program did
  ([ADR-0098](docs/adr/0098-locks-orderings-and-threads.md), increment 1).

### Added

- C's own types at the FFI boundary: the C the compiler writes for calls between the program and
  C names the types of `std::ffi` as C does (`char`, `long`) and each `@repr(C)` type by its name,
  and callbacks reach C through trampolines with those types. A callback a C library calls with
  `const char *`, `long` or a pointer to one of its structs, as SDL's do, is now compatible C, and
  `--sanitize` no longer reports it. `strerror`, `fwrite` and `fread` are declared with C's types
  in `std::sys` ([ADR-0105](docs/adr/0105-c-types-at-the-ffi-boundary.md)).
- The macros of C headers in `bindgen`: `#define`s whose bodies are constants become `pub const`s
  of integers, floats, strings and characters, computed from other macros, enumeration constants
  and casts. `nova bindgen <header>` writes the bindings of a header once, as `bindgen-cli`. With
  them S4 is done: the harness generates and uses the bindings of the system's C library
  ([ADR-0103](docs/adr/0103-build-scripts-and-binding-generation.md), increment 5).
- `bindgen`, as Rust's: build scripts generate Sagitario declarations from C headers with
  `new bindgen::Builder().header("lib.h").allow_prefix("LIB_").generate()`, which preprocesses
  them with clang or gcc and writes `extern "C"` blocks with variadic functions and C globals,
  `@repr(C)` structs and unions, opaque types, enumerations as constants and nullable function
  pointers, leaving out with a comment what it cannot translate. A C function the program
  declares that no library defines is now an error of the program when linking, with how to link
  its library, and no longer an internal compiler error. The editor gains the snippet `bindgen`
  ([ADR-0103](docs/adr/0103-build-scripts-and-binding-generation.md), increment 4).
- Build scripts, as Cargo's `build.rs`: a package's `build.sag`, which Nova compiles and runs
  before the package with `OUT_DIR`, `TARGET`, `HOST`, `PROFILE` and the package's name, version
  and directory in its environment. Its output's `nova::link-lib`, `nova::link-search` and
  `nova::link-source` add to `[link]`, `nova::rerun-if-changed` and `nova::rerun-if-env-changed`
  say when it runs again, and `nova::warning` shows a warning. Each package's
  `env!("OUT_DIR")` is its own script's directory, through the new options `--env` and
  `--dependency-env` that Nova passes to `sag`. The editor gains the snippet `buildscript`
  ([ADR-0103](docs/adr/0103-build-scripts-and-binding-generation.md), increment 3).
- Rust's built-in macros: `include!("file.sag");` puts the items of a file in the module, as
  `include!(concat!(env!("OUT_DIR"), "/bindings.sag"))` will for build scripts;
  `include_str!` and `include_bytes!` give the text and the bytes of a file; `concat!` joins
  literals, and `env!` reads a variable of the compiler's environment. They are expanded when the
  file is parsed. Programs cannot define macros. The editor colors and completes them, and the
  language server gives them as `macro` semantic tokens, as rust-analyzer does
  ([ADR-0103](docs/adr/0103-build-scripts-and-binding-generation.md), increment 2).
- Calls of variadic C functions, as Rust's: `fn printf(format: *const c_char, ...) -> c_int;` in
  an `extern "C"` block, whose extra arguments keep their types; those C promotes (`u8`, `bool`,
  `f32`...) are errors that give the cast. Opaque C types, as bindgen writes them: a `@repr(C)`
  struct whose only field is `prv _unused: [u8; 0]`, used only behind pointers. `std::sys` calls
  `fcntl` itself, so the runtime loses its two C helpers for sockets, and the sockets of macOS and
  iOS are now closed in the programs a program starts. The editor colors `...` and gains the
  snippets `externvariadic` and `opaque`
  ([ADR-0103](docs/adr/0103-build-scripts-and-binding-generation.md), increment 1).
- Widths and precisions read from bindings in format strings, `{x:>width$}`, `{pi:.digits$}`, as
  Rust's inline arguments, and the debug form of an interface value, which is that of the value
  it holds instead of `dyn Name`; the grammar colors the bindings. With them every row of the
  audit of S2 is closed, and S2 is done ([ADR-0101](docs/adr/0101-the-rest-of-s2.md), increment 5).
- The prelude's `BitAnd`, `BitOr`, `BitXor`, `Not`, `Shl` and `Shr`: `&`, `|`, `^`, `!`, `<<`, `>>`
  and their `op=` on structs, enums, classes and type parameters implementing them, as Rust's;
  integers and `bool` implement them, so generic code bounded by them takes numbers too
  ([ADR-0101](docs/adr/0101-the-rest-of-s2.md), increment 4).
- `std::hash` with `Hasher` and `DefaultHasher`, Rust's SipHash-1-3, whose hashes are Rust's for
  the same bytes; and `Error::source`, the error that caused one, `None` by default
  ([ADR-0101](docs/adr/0101-the-rest-of-s2.md), increment 3).
- `contains` and `is_empty` of a range written directly (`(1..=9).contains(x)`); `std::ptr` with
  `null`, `null_mut`, `read`, `write`, `copy`, `copy_nonoverlapping` and `write_bytes`; and
  `path::components`, `has_root`, `is_absolute`, `is_relative`, `with_extension`,
  `with_file_name`, `strip_prefix`, `starts_with` and `ends_with`, with `path::Component`
  ([ADR-0101](docs/adr/0101-the-rest-of-s2.md), increment 2).
- `to_le_bytes`, `to_be_bytes` and `to_ne_bytes` of every integer type, and `from_le_bytes`,
  `from_be_bytes`, `from_ne_bytes` and `from_str_radix`; the operators `+`, `-`, `*` and `/` of
  `Duration`, and its `checked_` and `saturating_` arithmetic, `mul_f64`, `div_f64`,
  `from_secs_f64` and the `f32` ones, as Rust's
  ([ADR-0101](docs/adr/0101-the-rest-of-s2.md), increment 1).
- `sag lsp` completes `net` after `std::`.
- The options of the sockets of `std::net`, as Rust's: read and write time outs,
  `set_nonblocking`, `set_ttl`, `take_error` and `try_clone`; `TcpStream::connect_timeout`,
  `set_nodelay` and `peek`; and `UdpSocket::set_broadcast`, `peek` and `peek_from`
  ([ADR-0100](docs/adr/0100-networking.md), increment 4).
- `std::net::UdpSocket` with `bind`, `send_to`, `recv_from`, `connect`, `send`, `recv`,
  `peer_addr` and `local_addr`, `Send` and closed by `Drop`
  ([ADR-0100](docs/adr/0100-networking.md), increment 3).
- `std::net::TcpListener` and `TcpStream`, blocking sockets on `Read` and `Write` that are `Send`
  and close by `Drop`, `Shutdown`, and `ToSocketAddrs`, which the compiler implements for `String`
  (`"localhost:8080"`, looked up with the system's resolver) and the pairs of a name or an IP
  address and a `u16` port. A program using them links Winsock on Windows
  ([ADR-0100](docs/adr/0100-networking.md), increment 2).
- `std::net` with `Ipv4Addr`, `Ipv6Addr`, `IpAddr`, `SocketAddrV4`, `SocketAddrV6`,
  `SocketAddr` and `AddrParseError`: Rust's texts (IPv6 as RFC 5952 writes it), parsing with
  `try_from`, order, hashing and constants such as `Ipv4Addr::LOCALHOST`
  ([ADR-0100](docs/adr/0100-networking.md), increment 1).
- Constants of structs, enums and classes, `pub const NAME: Type = value;`, named `Type::NAME`
  and `Self::NAME`, as Rust's associated constants; `sag lsp` completes them after `Type::` and
  `Self::`, lists them in the outline and colors them in paths, and the snippet `typeconst`
  declares one ([ADR-0099](docs/adr/0099-associated-and-composite-constants.md), increment 2).
- Constants that are structs, enums, `Option`, `Result`, arrays and tuples of constants, written
  as literals, and the parts of a constant (`ORIGIN.x`, `PAIR.0`, `PRIMES[3]`) in other
  constants ([ADR-0099](docs/adr/0099-associated-and-composite-constants.md), increment 1).
- `std::thread::Builder` with `name` and `stack_size`, whose `spawn` returns an `io::Error` when
  the system cannot start the thread and is checked as `thread::spawn`; `thread::current`,
  `Thread` with `id`, `name` and `unpark`, `ThreadId`, `JoinHandle::thread`, `park`,
  `park_timeout` and `yield_now`. The system gets the name of each thread
  ([ADR-0098](docs/adr/0098-locks-orderings-and-threads.md), increment 4).
- `Channel::bounded(capacity)`, whose `send` waits for room, or with 0 for a receiver, as Rust's
  `sync_channel`; and `Channel::try_send`, `try_receive` and `receive_timeout`, with
  `TrySendError`, `TryReceiveError` and `ReceiveTimeoutError`
  ([ADR-0098](docs/adr/0098-locks-orderings-and-threads.md), increment 3).
- `Mutex::lock` and `try_lock`, with a `MutexGuard` that releases the lock when it goes, as
  Rust's; `std::sync::Condvar`, `RwLock` with its read and write guards, and `Barrier`; and
  `TryLockError`, `WaitTimeoutResult` and `BarrierWaitResult`
  ([ADR-0098](docs/adr/0098-locks-orderings-and-threads.md), increment 2).
- `std::sync::Ordering` and `fence`; `AtomicI8`, `AtomicU8`, `AtomicI16`, `AtomicU16`,
  `AtomicIsize` and `AtomicPtr<T>`; `fetch_and`, `fetch_or`, `fetch_xor`, `fetch_nand`,
  `fetch_max`, `fetch_min`, `fetch_update` and `compare_exchange_weak` on the atomics, and
  `fetch_not` on `AtomicBool`. A mismatch between two types of the same name notes that they
  differ ([ADR-0098](docs/adr/0098-locks-orderings-and-threads.md), increment 1).

- `std::process::abort` and `id`; `std::env::vars`, `set_var` and `remove_var` (`unsafe`, as in
  Rust 2024), `set_current_dir`, `current_exe`, `temp_dir` and `home_dir`, and the constants
  `env::OS`, `ARCH` and `FAMILY`; `sag lsp` completes `exit` and `abort` after `std::process::`
  ([ADR-0097](docs/adr/0097-processes-and-environment.md), increment 3).
- `std::process::Stdio` (`inherit`, `piped`, `null`, and `from` a `File`) and `Command::stdin`,
  `stdout` and `stderr`; `Command::spawn` and `Child`, with its pipes as the fields `stdin`,
  `stdout` and `stderr` (`ChildStdin` is a `Write`, the others `Read`), `id`, `wait`, `try_wait`,
  `kill` and `wait_with_output`; and `File::try_clone` and `as_raw_handle`
  ([ADR-0097](docs/adr/0097-processes-and-environment.md), increment 2).
- `take` and `replace` of an `Option` place, as Rust's: `child.stdin.take()` takes the value out of
  a `var` field and leaves `None` ([ADR-0097](docs/adr/0097-processes-and-environment.md),
  increment 1).
- `fs::metadata`, `symlink_metadata` and `File::metadata`, with the size, kind, read-only flag and
  times of a file as `SystemTime`s; `fs::remove_dir_all` and `fs::canonicalize`; `time::UNIX_EPOCH`
  and `SystemTime + Duration`; and `io::ErrorKind::Unsupported`
  ([ADR-0096](docs/adr/0096-streams-files-and-drop.md), increment 4).
- `std::io::BufRead`, `BufReader` and `BufWriter`, which flushes on its `drop`, and `io::stdin()`,
  `stdout()` and `stderr()` as readers and writers of bytes, with `Stdout::lock`
  ([ADR-0096](docs/adr/0096-streams-files-and-drop.md), increment 3).
- `std::io::Read` and `Write`, as Rust's, on slices, with `read_to_end`, `read_to_string`,
  `read_exact`, `write_all` and `write_str`; `std::fs::File`, which closes by `Drop` or `close`,
  with `seek`, `set_len` and `sync_all`; `OpenOptions` and `SeekFrom`
  ([ADR-0096](docs/adr/0096-streams-files-and-drop.md), increment 2).
- `Drop`, for classes: `drop` runs when the last strong reference to an instance goes, before its
  fields are released, and keeping `self` alive there panics; instances whose last references go
  together are now freed in the order those references went, so a list's elements go first to
  last ([ADR-0096](docs/adr/0096-streams-files-and-drop.md), increment 1).
- `std::collections::BinaryHeap`, as Rust's: a priority queue, the largest first, with
  `with_compare` for any other order, `into_sorted_list` and the debug form of its heap
  ([ADR-0095](docs/adr/0095-collections.md), increment 5).
- `std::collections::BTreeMap` and `BTreeSet`, as Rust's: B-trees ordered by their keys, with the
  methods of `Map` and `HashSet` in order, `first_key_value`, `pop_first`, `pop_last`, `range`,
  `range_from` and `range_to`, `for (key, value) in map`, comparison, order and hash by their
  entries, and the debug forms `{"a": 1}` and `{1, 2}`
  ([ADR-0095](docs/adr/0095-collections.md), increment 4).
- `std::collections::VecDeque`, as Rust's: a ring buffer with both ends, `deque[i]`, an iterator
  of two ends, comparison, order and hash by its elements, and the debug form `[1, 2]`; the
  method of an interface a type implements may return a subclass of the class declared
  ([ADR-0095](docs/adr/0095-collections.md), increment 3).
- `HashSet` in the prelude, as Rust's, keeping insertion order, with `==` by value, the operations
  between sets and the debug form `{1, 2}`; and `Map::get_key_value` and `Map::remove_entry`
  ([ADR-0095](docs/adr/0095-collections.md), increment 2).
- `Iterable` and `FromIterator`, as Rust's `IntoIterator` and `FromIterator`: `for` iterates over
  any type implementing `Iterable`, `collect()` makes any type implementing `FromIterator`, and
  lists, arrays, iterators, strings, maps and results implement them for bounds; and the module
  `std::collections` ([ADR-0095](docs/adr/0095-collections.md), increment 1).
- `Index` and `IndexMut`, as Rust's: `value[i]` calls `index`, and `value[i] = v`,
  `value[i] += v` or `value[i].field = v` read the element, change it and write it back with
  `index_set`; a type may be indexed by several types, and `map[key]` gives the value of a key
  or panics ([ADR-0094](docs/adr/0094-conversions-and-defaults.md), increment 5).
- The prelude functions `min` and `max` of any `Ord` type, `clamp` on every `Ord` type, and
  Rust's methods of `Ordering`: `reverse`, `then`, `then_with`, `is_eq`, `is_lt`...
  ([ADR-0094](docs/adr/0094-conversions-and-defaults.md), increment 4).
- `Default`, as Rust's: implemented by hand or with the attribute `@default` before a struct (the
  default of each field) or an enum (the variant marked `@default`), and by numbers, `bool`,
  `char`, strings, lists, maps, `Option`, tuples and arrays; `Type::default()` and
  `T::default()`, `unwrap_or_default` on `Option` and `Result`, `mem::take`, `mem::replace` and
  `mem::swap`, and struct update, `Config { verbose: true, ..Config::default() }`
  ([ADR-0094](docs/adr/0094-conversions-and-defaults.md), increment 3).
- `From`, `Into`, `TryFrom` and `TryInto`, as Rust's: `Type::from(x)` chooses the implementation
  by the type of `x`, every type is `From` itself, `x.into()` converts into the type expected or
  that of a bound `T: Into<U>`, and the prelude has Rust's conversions between numbers
  (`i64::from(5i32)`, `u8::try_from(300i32)`), characters, strings, lists and maps; `?` converts
  an error through `From` ([ADR-0094](docs/adr/0094-conversions-and-defaults.md), increment 2).
- Static functions in interfaces, such as `fn zero() -> Self;`, with or without a default,
  implemented by a type's body or an `impl ... for` block and called as `Type::zero()`,
  `Self::zero()` or `T::zero()` for a type parameter bounded by the interface
  ([ADR-0094](docs/adr/0094-conversions-and-defaults.md), increment 1).
- Rust's adapters and consumers of iterators (`enumerate`, `zip`, `chain`, `flat_map`, `flatten`,
  `take_while`, `sum`, `min`, `max`, `position`, `partition`, `unzip`...), `peek` on every
  iterator, `DoubleEndedIter<T>` with `rev` and `next_back` for those of lists, slices, ranges
  and strings, and `collect` into a `Map`, a `String` or a `Result` when the context expects one
  ([ADR-0093](docs/adr/0093-everyday-methods.md), increment 5).
- `Map::update`, `upsert` and `get_or_insert`, which do what Rust's `entry` is used for, with
  `retain`, `extend`, `iter` (pairs) and `Map::from` a list of pairs
  ([ADR-0093](docs/adr/0093-everyday-methods.md), increment 4).
- Rust's methods of lists (`extend`, `append`, `retain`, `dedup`, `drain`, `split_off`,
  `binary_search`, `sort_by_key`, `windows`, `chunks`, `min`, `max`, `List::with_capacity`...), and
  those of slices, which arrays and lists have too (`contains`, `sort`, `reverse`, `fill`, `swap`,
  `copy_from_slice`...); lists and arrays are compared, ordered and hashed by their elements, and
  maps compared by their entries ([ADR-0093](docs/adr/0093-everyday-methods.md), increment 3).
- Rust's methods of `String` (`rfind`, `split_once`, `strip_prefix`, `split_whitespace`,
  `char_indices`, `to_uppercase` with Unicode's full mappings...), `String::from_utf8` and
  `from_utf16`, the Unicode properties of `char` (`is_alphabetic`, `is_numeric`, `is_uppercase`...)
  and `char::from_digit`, and `StringBuilder::insert`, `truncate` and `pop`
  ([ADR-0093](docs/adr/0093-everyday-methods.md), increment 2).
- `assert`, `assert_eq`, `assert_ne`, `unreachable` and `todo`, which panic at the line of their
  call, and Rust's other methods of `Option` and `Result` (`ok_or`, `or`, `and`, `xor`, `map_or`,
  `zip`, `flatten`, `ok`, `err`, `expect_err`...); `Option` and `Result` are compared, ordered and
  hashed when the values they hold are ([ADR-0093](docs/adr/0093-everyday-methods.md), increment 1).
- Tuples: `(i32, String)` types, `(a, b)` values and `t.0` reads, compared, ordered and hashed
  when their elements are, so they sort and key maps, with a debug form `(1, "a")`
  ([ADR-0092](docs/adr/0092-tuples.md), increment 1), and tuple patterns, `(a, b)` and
  `(first, .., last)`, in `match`, `if let`, `while let`, `const`, `var` and `for`
  (`for (index, value) in pairs`), with matches on tuples checked for every case (increment 2).
- Global state: `static` and `static var`, one `Send` value for the program computed on its first
  use ([ADR-0083](docs/adr/0083-global-state.md)), and `@thread_local static var`, one value per
  thread.
- Conditional compilation with `@cfg(...)` on items, `use` and the functions and statics of
  `extern` blocks: `windows`, `unix`, `target_os`, `target_arch`, `target_pointer_width`, `all`,
  `any` and `not` ([ADR-0085](docs/adr/0085-conditional-compilation-and-std-time.md)).
- C's global variables in `extern "C"` blocks, `static NAME: T;` and `static var NAME: T;`, used
  inside `unsafe` ([ADR-0086](docs/adr/0086-c-globals-and-std-io.md)).
- Bitwise operators and shifts, as Rust's: `&`, `|`, `^`, `!` on integers, `<<` and `>>`, with
  `&=`, `|=`, `^=`, `<<=` and `>>=` and Rust's precedence; a shift by the width or more panics in
  debug builds ([ADR-0088](docs/adr/0088-bitwise-operators-and-integer-methods.md)).
- The integer methods spec 2 promised: `wrapping_*`, `checked_*` (giving `Option`) and
  `saturating_*`, with `count_ones`, `leading_zeros`, `trailing_zeros`, `rotate_left`,
  `rotate_right`, `swap_bytes` and `is_power_of_two`; and `i32::MIN`, `i32::MAX` and those of
  every integer type.
- `Display`, a prelude interface with `to_string()`: format strings interpolate the structs,
  enums and classes implementing it, and numbers, `bool`, `char` and `String` have
  `to_string()` ([ADR-0089](docs/adr/0089-display-debug-and-format-specifiers.md)).
- Format specifiers, Rust's mini-language after a `:` in the braces of a format string:
  width, fill and alignment, `+`, zero padding, precision, and `x`, `X`, `o`, `b`, `e`, `E`
  (`f"{price:>8.2}"`, `f"{mask:#06x}"`).
- The debug form of any value, `{x:?}` and the multi-line `{x:#?}`, which the compiler gives every
  type: `Point { x: 1, y: 2 }`, `Some(1)`, `[1, 2]`, `{"a": 1}`, strings with Rust's escapes, and
  cycles of classes cut as `Name { .. }`.
- `std::hint::black_box`, and a suite of nine benchmarks against Rust, C, C#, JavaScript and
  Python, run with `sag run tools/bench.sag` ([ADR-0090](docs/adr/0090-benchmarks.md)); with
  `--race`, the results as a race of the languages, an HTML page, and with `--video` an MP4 when
  `ffmpeg` is installed. The runner records the peak memory of each program, and
  [`docs/benchmarks.md`](docs/benchmarks.md) the first results and the work they point at.
- An audit of the standard library against Rust's ([`docs/std-audit.md`](docs/std-audit.md)), the
  plan of S2, with a suite of benchmarks against Rust, C, C#, JavaScript and Python.
- The language server completes `@cfg` conditions, attributes and their representations, and
  shows the types of constants; the VS Code extension gains snippets for `@thread_local`, `@cfg`
  and C globals.
- `tools/vscode.sag` packages the VS Code extension and installs it in the local VS Code.

### Changed

- `io::ErrorKind` has a variant more, `Unsupported`, so a `match` over it that names every kind
  needs its arm ([ADR-0096](docs/adr/0096-streams-files-and-drop.md), increment 4).
- `rev()` works on any iterator with two ends, and is an error on the others, as in Rust;
  `list.iter()` and the other iterators with two ends have the type `DoubleEndedIter<T>`, which
  is an `Iter<T>` ([ADR-0093](docs/adr/0093-everyday-methods.md), Q627).
- The standard library is written in Sagitario on the C API of each system, gathered in the
  private module `std::sys`, instead of C in the runtime (S1 of the
  [roadmap](docs/roadmap-v2.md)): `std::time`, `std::env`, `std::path`, `std::io`, `std::fs`,
  `std::process::Command`, the dynamic libraries of `std::ffi::Library`, `std::sync` and
  `std::thread`. About 2,000 lines of C leave the runtime. On Windows, `Command` reads the pipes
  of the child with overlapped I/O instead of threads. The atomics of `std::sync` are `_Atomic`
  fields of an intrinsic type of the standard library, `std::sys::Atomic`, instead of runtime
  objects ([ADR-0087](docs/adr/0087-atomic-storage-and-std-sync.md)).
- Generated programs count references through inline functions, reach list elements with the
  size of their type as a constant, push inline while a list has room, and compare primitives in
  generic code with C's own operators.
- A pass over SIR removes the retain and the release of a temporary moved into a binding, a
  field or an element, and pairs of counts with nothing between them
  ([ADR-0091](docs/adr/0091-fewer-reference-counts.md)).
- `List::sort` and `sort_by` are a merge sort through one buffer with runs sorted by insertion,
  about 5 times faster; `Map` compares keys only when their hashes match, and a `Channel` wakes
  its receivers only when one waits.
- The standard library's C declarations go into a C file of their own (`sag_std_ffi.c`), so they
  never clash with a program's, and it declares only the C functions the program uses.
- The roadmap is replanned as a complete language with a standard library like Rust's; the game
  engine moves to a project of its own ([ADR-0084](docs/adr/0084-a-complete-language-and-the-engine-apart.md)).

### Fixed

- An interface that cannot be a type (one that mentions `Self`, has methods with type parameters
  or, now, static functions) written as a type inside a function body is an error (`E0329`), as
  it already was in declarations; it was accepted there.
- On Windows, `Command` runs a program given by a relative path written with `/`
  (`build/tool.exe`), which `CreateProcessW` did not find.
- A statement starting with a block-like expression (`if`, `match`, `unsafe { }`, a block) ends at
  its `}`: `unsafe { f() } * p` no longer parses as a multiplication. Another operator there is
  an error (`E0100`) that asks for parentheses.

### Fixed

- A `for` over an iterator that is a part of another value, such as `option.unwrap()`, was an
  internal compiler error.

## [0.20.0] - 2026-11-10

### Added

- A complete FFI: C layouts with `@repr(C)` structs, unions and enums, C function pointers,
  `&raw const` and `&raw mut`, buffers in lists, `std::mem` and dynamic libraries with
  `std::ffi::Library` ([ADR-0078](docs/adr/0078-ffi-without-c-shims.md)).
- References in parameters (`&T`, `&mut T`) with a borrow checker, slices and borrowed lists
  ([ADR-0079](docs/adr/0079-references-and-borrowing.md)); `fs::write_bytes` for slices of bytes.
- Operator overloading through interfaces: `impl ... for` blocks, several implementations of an
  interface, arithmetic through `Add`, `Sub`, `Mul`, `Div`, `Rem` and `Neg`, and comparison of
  structs, enums and type parameters through `Eq` and `Ord`
  ([ADR-0081](docs/adr/0081-operator-overloading.md)).
- Format strings `f"..."` with `{expr}` interpolation
  ([ADR-0082](docs/adr/0082-format-strings.md)).
- `std::math`: constants, methods of numbers, non-finite float constants, powers, roots,
  logarithms and trigonometry ([ADR-0080](docs/adr/0080-std-math-trigonometry.md)).
- `tools/version.sag` changes the version in every file that holds it.
- The language server completes the methods of numbers, slices, pointers and references, and the
  editor highlights the expressions inside format strings and templates.

### Fixed

- The threads of `spawn_blocking` jobs that end after their executor are detached.

## [0.10.0] - 2026-10-28

### Added

- `async fn`, `await` and tasks with `std::task` (F8), also as methods, generic functions and
  values; blocking work on threads with `task::spawn_blocking`
  ([ADR-0073](docs/adr/0073-spawn-blocking.md)).
- The F8 acceptance program, an asynchronous pipeline of files.

## [0.9.0] - 2026-10-28

### Added

- `nova update self` updates Sagitario to the latest release.
- The VS Code extension is published with the releases
  ([ADR-0071](docs/adr/0071-vscode-extension-with-the-releases.md)).
- One-command installers for each platform ([ADR-0069](docs/adr/0069-installers.md)).

### Fixed

- Nova hashes downloaded archives with .NET on Windows instead of `Get-FileHash`.
- `c_char` is `u8` when the compiler runs on Linux for ARM.

## [0.8.0] - 2026-10-27

The first release: the language through F6, with a compiler written in Sagitario.

### Added

- The core language (F1): variables, operators, control flow, ARC strings, structs, methods,
  fixed arrays, enums, `match`, `Option`, `Result` and `?`, the `String` API, `StringBuilder`,
  `panic`, modules with `use` and visibility, and an FFI with `extern "C"`, `unsafe`, raw
  pointers, `@export` and `@repr(C)`; diagnostics with codes, suggestions and colors.
- Classes (F2) with ARC, inheritance with `override`, `abstract` and dynamic dispatch, `weak`
  fields, interfaces with fields and default methods, monomorphized generics with bounds,
  closures and function values, and builds with sanitizers.
- The standard library (F3): `std::env`, `std::io`, `std::fs`, `std::path`, `std::time` and
  `std::process` with `Command`.
- Nova (F3), the package manager: `Nova.toml`, `new`, `init`, `build`, `run`, `check`, `clean`,
  dependencies by path, libraries (`.a`, `.so`, `.dylib`, `.dll`) with a C header, `[link]` for
  C libraries and sources, and cross-compilation to Android and iOS with `--target`.
- Concurrency (F4): `std::thread` with an inferred `Send`, `Mutex`, atomics and channels in
  `std::sync`.
- Module-level constants, `include_str`, `if let` and `while let` (F5).
- The compiler in Sagitario (F5, F6): lexer, parser, resolver, type checker, SIR and C code
  generation, with dumps of each pass, built from a seed of the C it generates for itself and
  checked to reach a fixed point.
- The language server `sag lsp` with semantic tokens, hover and definitions, and the VS Code
  extension with highlighting, snippets and a file icon (F6).
- Nova and the test harness written in Sagitario.
- Release archives built from the seed and published when a tag is pushed
  ([ADR-0068](docs/adr/0068-releases-and-distribution.md)).
- MSVC `cl` support.

### Removed

- The first compiler, written in Rust: Sagitario compiles itself. `sagc` is renamed `sag`, and the
  standard library moves to `compiler/std`.

[Unreleased]: https://github.com/Juanestban/sagitario/compare/v0.20.0...HEAD
[0.20.0]: https://github.com/Juanestban/sagitario/compare/v0.10.0...v0.20.0
[0.10.0]: https://github.com/Juanestban/sagitario/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/Juanestban/sagitario/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/Juanestban/sagitario/releases/tag/v0.8.0
