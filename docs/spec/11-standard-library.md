# 11. Standard library (F3)

Rationale: [ADR-0037](../adr/0037-standard-library-env-io-process.md),
[ADR-0038](../adr/0038-file-system-and-paths.md), [ADR-0039](../adr/0039-time-and-commands.md),
[ADR-0045](../adr/0045-threads-and-send.md), [ADR-0046](../adr/0046-mutex-and-atomics.md),
[ADR-0047](../adr/0047-channels-and-thread-utilities.md),
[ADR-0076](../adr/0076-std-math-and-methods-of-numbers.md),
[ADR-0077](../adr/0077-std-math-powers-and-logarithms.md),
[ADR-0078](../adr/0078-ffi-without-c-shims.md),
[ADR-0080](../adr/0080-std-math-trigonometry.md).

The standard library is the set of modules under the path root `std`. A program imports them
with `use` like its own modules (`use std::io;`, `use std::env::args;`) or names them in full
(`std::process::exit(1)`). Only the modules a program names are compiled, and of those only the
functions it uses.

| Module | Contents | Milestone |
|---|---|---|
| `std::ffi` | C types, `CString`, `string_from_c`, `Library` ([FFI](10-ffi.md)) | F1, G1 |
| `std::env` | Command-line arguments and environment variables | F3 |
| `std::io` | Standard input and output with errors as values; `io::Error` | F3 |
| `std::fs` | Files and directories | F3 |
| `std::hint` | `black_box`, for benchmarks | S2 |
| `std::math` | Constants of `f64`, and the methods of floats as functions | G1 |
| `std::mem` | Sizes and alignments of types, and zeroed values for C | G1 |
| `std::path` | Joining paths and taking them apart | F3 |
| `std::process` | `exit`, running programs | F3 |
| `std::sync` | `Mutex`, atomic values and channels, shared between threads | F4 |
| `std::task` | Tasks of `async fn`, `sleep` and `block_on` ([`async` and `await`](12-async.md#stdtask)) | F8 |
| `std::thread` | Threads | F4 |
| `std::time` | Durations, clocks and `sleep` | F3 |

Naming a module that does not exist (`std::net`) is an error.

```sag
use std::env;
use std::io;

fn main() -> Result<(), io::Error> {
  const name = env::args().iter().skip(1).find((arg) => !arg.is_empty()).unwrap_or("world");
  io::println(`Hello, ${name}!`)?;
  Ok(())
}
```

## `std::collections`

Rust's collections beyond the prelude's `List`, `Map` and `HashSet`
([ADR-0095](../adr/0095-collections.md)), written in Sagitario: `VecDeque`, `BTreeMap`,
`BTreeSet` and `BinaryHeap`.

```sag
use std::collections::VecDeque;

const queue = VecDeque::from(List::from([1, 2, 3]));

queue.push_front(0);
queue[1] = 10;
println(f"{queue.pop_back():?} {queue:?}");   // Some(3) [0, 10, 2]
```

**`VecDeque<T>`** (Q648), a queue with two ends in a ring buffer:

| Method | Result |
|---|---|
| `new VecDeque<T>()`, `VecDeque::with_capacity(n)`, `VecDeque::from(list)` | A queue, the first element of the list at the front |
| `push_back(v)`, `push_front(v)` | `()` |
| `pop_back()`, `pop_front()`, `front()`, `back()`, `get(i)` | `Option<T>` |
| `len()`, `is_empty()`, `capacity()`, `clear()` | |
| `iter()` | `DoubleEndedIter<T>`, from the front; `rev()` from the back |
| `extend(iter)`, `retain(keep)`, `truncate(len)` | `()` |
| `swap(i, j)`, `rotate_left(n)`, `rotate_right(n)` | `()`; panic outside the queue, with Rust's messages |
| `deque[i]`, `deque[i] = v` | through `Index` and `IndexMut`; outside the queue they panic (`Out of bounds access`) |

- It implements `Iterable`, `FromIterator` and `Default`, and, when its elements do, `Eq`, `Ord`
  and `Hash`, compared, ordered and hashed as a list of its elements, which the compiler gives as
  for `Map`. Its debug form is `[1, 2]`.
- Rust's `contains` needs `T: Eq` on the method alone, which Sagitario cannot write:
  `deque.iter().any((x) => x == value)` stands for it. `make_contiguous` is not needed, since the
  ring is never lent.

```sag
use std::collections::{BTreeMap, BTreeSet};

const scores = BTreeMap::from(List::from([("bob", 3), ("ada", 5)]));

for (name, score) in scores {
  println(f"{name} {score}");                    // ada 5, then bob 3
}

println(f"{scores.range_from("b").collect():?} {BTreeSet::from(List::from([3, 1, 2])):?}");   // [("bob", 3)] {1, 2, 3}
```

**`BTreeMap<K: Ord, V>`** (Q649), a map ordered by its keys, in a B-tree of nodes of 5 to 11 keys:

- The methods of `Map`, in the order of the keys: `new`, `BTreeMap::from(pairs)`, `insert(k, v) ->
  Option<V>`, `get`, `get_key_value`, `contains_key`, `remove`, `remove_entry`, `update`, `upsert`,
  `get_or_insert`, `retain`, `extend`, `len`, `is_empty`, `clear`, `iter()`, `keys()` and
  `values()` (each a `DoubleEndedIter`), and `map[key]` through `Index`, which panics without the
  key (`no entry found for key`).
- The ordered ones: `first_key_value()` and `last_key_value()` (`Option<(K, V)>`), `pop_first()`
  and `pop_last()`, and, since ranges are not values, `range(low, high)` (`low` included, `high`
  not), `range_from(low)` and `range_to(high)`, each a `DoubleEndedIter<(K, V)>`.
- `for (key, value) in map` goes over its pairs, as Rust's, since it implements
  `Iterable<(K, V)>`; it implements `FromIterator<(K, V)>` and `Default` too. Its iterators go over
  the entries a walk of the tree gathers when they are made, not lazily as Rust's.

**`BTreeSet<T: Ord>`** (Q650), a set ordered by its values, on a `BTreeMap`: the methods of
`HashSet`, in order (`union` and `symmetric_difference` give their values in order too), with
`first`, `last`, `pop_first`, `pop_last`, `range`, `range_from` and `range_to`.

Both compare, order and hash as a list of their entries when these allow it, as the compiler
gives `VecDeque`, and their debug forms are `{"a": 1}` and `{1, 2}`.

**`BinaryHeap<T: Ord>`** (Q651), a priority queue in a binary heap, the largest first:

| Method | Result |
|---|---|
| `new BinaryHeap<T>()`, `BinaryHeap::with_capacity(n)`, `BinaryHeap::from(list)` | A heap ordered by `Ord`; `from` builds it in linear time |
| `BinaryHeap::with_compare(compare)` | A heap ordered by `compare: fn(T, T) -> Ordering`, the largest by it first: `(a, b) => b.compare(a)` gives the smallest first, which Rust's `Reverse` does |
| `push(v)`, `pop() -> Option<T>`, `peek() -> Option<T>` | The largest is popped and peeked at |
| `len()`, `is_empty()`, `clear()`, `extend(iter)`, `retain(keep)` | |
| `iter()` | The elements in the order of the heap, which is no order |
| `into_list()`, `into_sorted_list()` | A list of the elements in the order of the heap, or from the smallest, as Rust's `into_vec` and `into_sorted_vec`; the heap stays, since nothing moves |

- It implements `Iterable`, `FromIterator` and `Default`, and is not compared by value: `==`
  compares instances, as of any class. Its debug form is the list of its heap, `[9, 2, 4]`.

## `std::env`

| Function | Result |
|---|---|
| `args() -> List<String>` | The command-line arguments, starting with the program as the operating system started it (usually its path) |
| `variable(name: String) -> Option<String>` | The value of an environment variable, or `None` if it is not set |
| `vars() -> List<(String, String)>` | Every environment variable, as its name and its value, in the order of the environment |
| `unsafe set_var(name: String, value: String)` | Sets a variable, for this program and the programs it starts |
| `unsafe remove_var(name: String)` | Removes a variable; one that is not set is no error |
| `set_current_dir(path: String) -> Result<(), io::Error>` | Changes the current working directory, which [`fs::current_dir`](#stdfs) gives |
| `current_exe() -> Result<String, io::Error>` | The full path of this program's file |
| `temp_dir() -> String` | The directory for temporary files |
| `home_dir() -> Option<String>` | The home directory of the user, or `None` if the system has none |

- Arguments and values that are not valid UTF-8 have each ill-formed sequence replaced by
  U+FFFD. On Windows they are read as UTF-16, so any Unicode text arrives intact.
- A variable name that is empty or contains `=` or a NUL character is never set: `variable`
  returns `None`.
- Arguments are not part of `main`'s signature ([errors](07-errors.md#program-entry-point-and-exit-codes)).
- `set_var` and `remove_var` are `unsafe`, as in Rust 2024: another thread, or C code, may read
  the environment at the same time, which systems other than Windows do not make safe. A name
  that is empty or contains `=` or a NUL, a value that contains a NUL, or a change the system
  refuses panics, as Rust's (`failed to set environment variable "A=B": Invalid argument`)
  ([ADR-0097](../adr/0097-processes-and-environment.md), Q668).
- `vars` gives a name that starts with `=` as it is, as Rust's: Windows keeps hidden variables of
  the shell such as `=C:`.
- `current_exe` reads `/proc/self/exe` on Linux and Android, asks `_NSGetExecutablePath` on Apple's
  systems and makes it absolute without links, and asks `GetModuleFileNameW` on Windows.
- `temp_dir` is `TMPDIR`, else `/tmp` (`/data/local/tmp` on Android); on Windows, what
  `GetTempPathW` gives, ending in `\` (`TMP`, `TEMP` or `USERPROFILE`, else the directory of
  Windows). It does not check that the directory exists.
- `home_dir` is `HOME` unless it is empty, else the one of the system's database of users
  (`getpwuid_r`), as Rust's; on Windows, `USERPROFILE` unless it is empty, else `None`.

The constants of the target the program was compiled for, as Rust's `std::env::consts`, are
written `env::OS`, since a module of `std` is one file:

| Constant | Values |
|---|---|
| `OS: String` | `"linux"`, `"macos"`, `"windows"`, `"android"`, `"ios"` |
| `ARCH: String` | `"x86_64"`, `"aarch64"`, `"arm"` |
| `FAMILY: String` | `"unix"`, `"windows"` |

They hold what [`@cfg`](09-modules.md#conditional-compilation-s1) tests, for code that prints or
compares them (`println(f"{env::OS} on {env::ARCH}")`).

## `std::io`

| Function | Result |
|---|---|
| `print(text: String)` | Writes `text` to stdout |
| `println(text: String)` | Writes `text` and `\n` to stdout |
| `eprint(text: String)` | Writes `text` to stderr |
| `eprintln(text: String)` | Writes `text` and `\n` to stderr |
| `flush()` | Writes out what stdout holds in its buffer |
| `read_line() -> Result<Option<String>, io::Error>` | The next line of stdin without its `\n` or `\r\n`; `None` at the end of the input |
| `read_to_string() -> Result<String, io::Error>` | The rest of stdin; `""` at the end of the input |
| `is_terminal(stream: io::Stream) -> bool` | Whether `io::Stream::Stdin`, `Stdout` or `Stderr` is connected to a terminal rather than to a file or a pipe |
| `read_exact(count: usize) -> Result<String, io::Error>` | Exactly `count` bytes of stdin, such as a message whose length is known; an input that ends before is an error of kind `UnexpectedEof` |

- The functions without a stated result return `Result<(), io::Error>`. The prelude's `print`,
  `println` and `eprintln` panic instead ([prelude](09-modules.md#prelude)); both write to the
  same streams, in order.
- Input that is not valid UTF-8 is an error of kind `InvalidData`; the bytes read are lost.
- stdin, stdout and stderr are read and written byte for byte on every system: `\n` is written as
  `\n`, also on Windows, and nothing is translated when reading
  ([ADR-0058](../adr/0058-language-server.md)).
- `is_terminal` is `false` for files, pipes and, on Windows, any device that is not a console,
  such as `NUL` ([ADR-0061](../adr/0061-terminals.md)).

### `io::Error` and `io::ErrorKind`

```sag
pub enum ErrorKind {
  NotFound,
  PermissionDenied,
  AlreadyExists,
  InvalidInput,
  InvalidData,
  UnexpectedEof,
  Unsupported,
  Other,
}
```

- `io::Error` is a class implementing the prelude's `Error` interface. `kind()` returns its
  `ErrorKind`; `message()` returns the operating system's description of the failure.
- `new io::Error(kind, message)` creates one, for code that reports its own I/O failures.
- Like any type implementing `Error`, it converts to `Error` with `?`
  ([errors](07-errors.md#the--operator)), and `main` may return `Result<(), io::Error>`.
- The kinds are the same on every operating system. Failures without a more precise kind are
  `Other`.

### Streams

Rationale: [ADR-0096](../adr/0096-streams-files-and-drop.md), Q656. Two interfaces, as Rust's,
let code read and write any source and destination of bytes, a [file](#stdfs) among them:

```sag
interface Read {
  fn read(self, buffer: &mut [u8]) -> Result<usize, Error>;
  // read_to_end, read_to_string and read_exact, with defaults
}

interface Write {
  fn write(self, bytes: &[u8]) -> Result<usize, Error>;
  fn flush(self) -> Result<(), Error>;
  // write_all and write_str, with defaults
}

fn copy<R: Read, W: Write>(from: R, to: W) -> Result<(), io::Error> {
  to.write_all(&from.read_to_end()?)
}
```

| Method | Result |
|---|---|
| `read(buffer: &mut [u8])` | `Result<usize, Error>`: the bytes put at the start of `buffer`; `0` at the end |
| `read_to_end()` | `Result<List<u8>, Error>`: every byte left |
| `read_to_string()` | `Result<String, Error>`: every byte left, as text; invalid UTF-8 is an error of kind `InvalidData` |
| `read_exact(buffer: &mut [u8])` | Exactly as many bytes as `buffer` holds; an end before is an error of kind `UnexpectedEof` |
| `write(bytes: &[u8])` | `Result<usize, Error>`: the bytes taken from the start of `bytes` |
| `flush()` | Writes out what a buffer holds |
| `write_all(bytes: &[u8])` | Every byte, in as many writes as it takes |
| `write_str(text: String)` | The bytes of `text`, which Rust writes with `write!` |

`SeekFrom`, an enum of `std::io`, says where [`seek`](#stdfs) moves: `Start(u64)`, `End(i64)` or
`Current(i64)`.

### Buffers and the standard streams

Rationale: [ADR-0096](../adr/0096-streams-files-and-drop.md), Q658 and Q660.

```sag
use std::fs::File;
use std::io;
use std::io::{BufReader, BufWriter, Write};

for line in new BufReader(File::open("input.txt")?).lines() {
  println(line?);
}

const out = new BufWriter(File::create("output.txt")?);

out.write_str("written when the buffer fills, on flush, or on its drop")?;

const lock = io::stdout().lock();   // stdout is this thread's until `lock` goes
```

- `BufRead`, an interface extending `Read`, has `read_line() -> Result<Option<String>, Error>`
  (the next line without its `\n` or `\r\n`, `None` at the end; bytes that are not UTF-8 are an
  error of kind `InvalidData`) and `lines() -> Iter<Result<String, Error>>`. Rust's `fill_buf`
  lends the buffer, which Sagitario cannot.
- `new BufReader(inner)` reads any `Read` through a buffer of 8 KiB, and
  `BufReader::with_capacity(n, inner)` through one of `n` bytes; it implements `Read` and
  `BufRead`, and a read as large as its buffer, when it holds nothing, goes straight to `inner`.
- `new BufWriter(inner)` and `BufWriter::with_capacity(n, inner)` write to any `Write` through a
  buffer, which reaches `inner` when it is full, on `flush()`, and on the writer's `drop`, which,
  as Rust's, loses an error there; bytes as many as the buffer holds go straight to `inner`. Its
  `drop` runs before its fields are released, so a `File` inside it is closed after.
- Both have `get_ref()`, the inner reader or writer, and `capacity()`.
- `io::stdin()` gives a `Stdin`, a `Read` and `BufRead` on the stream `io::read_line` reads, so
  the two can be mixed; its `read` gives what is available up to the end of a line, so a terminal
  is not waited on for more. `io::stdout()` and `io::stderr()` give a `Stdout` and a `Stderr`,
  `Write`s of bytes on the streams `print` writes.
- `Stdout::lock()` gives a `StdoutLock`, a `Write` holding the lock of stdout until its `drop`:
  every write of stdout takes that lock too, so other threads' `print` waits, and a series of
  writes through it is not interleaved.

## `std::fs`

Rationale: [ADR-0038](../adr/0038-file-system-and-paths.md). Paths are `String`s, relative to the
current directory unless they have a root. The functions with a `Result` fail with an
`io::Error` whose message names the operation and the path:
`cannot read "notes.txt": No such file or directory`.

| Function | Result |
|---|---|
| `read_to_string(path) -> Result<String, io::Error>` | The whole file |
| `read(path) -> Result<List<u8>, io::Error>` | The bytes of the whole file, whatever they are (an image, SPIR-V) |
| `write(path, text)` | Creates the file, or replaces its contents |
| `write_bytes(path, bytes: &[u8])` | The same with bytes, from any [slice](02-types.md#slices): `fs::write_bytes(path, &list)` |
| `append(path, text)` | Adds `text` at the end of the file, creating it if needed |
| `exists(path) -> bool` | Whether `path` names a file, a directory or anything else |
| `is_file(path) -> bool`, `is_dir(path) -> bool` | Whether it names a regular file, or a directory |
| `create_dir(path)` | Creates a directory; its parent must exist |
| `create_dir_all(path)` | Creates a directory and the missing ones above it; one that exists is not an error |
| `remove_dir(path)` | Removes an empty directory |
| `remove_file(path)` | Removes a file |
| `rename(from, to)` | Moves a file or directory, replacing `to` if it is a file that exists |
| `copy(from, to)` | Copies a file, replacing `to` if it exists |
| `read_dir(path) -> Result<List<String>, io::Error>` | The names of a directory's entries, without `.` and `..`, sorted by their bytes |
| `current_dir() -> Result<String, io::Error>` | The current working directory, as an absolute path |

- The functions without a stated result return `Result<(), io::Error>`.
- Contents are read and written byte for byte: `\r\n` is never translated. A file that is not
  valid UTF-8 is an error of kind `InvalidData` for `read_to_string`, never for `read`.
- `exists`, `is_file` and `is_dir` return `false` when the path cannot be examined.
- Using a directory as a file, or a file as a directory, fails the same way on every system:
  messages such as "Is a directory" and "Not a directory", of kind `Other`.
- Names that are not valid UTF-8 are listed by `read_dir` with each ill-formed sequence replaced
  by U+FFFD. On Windows, paths are passed to the system as UTF-16, so any Unicode name works.
- A path containing a NUL character is an error of kind `InvalidInput`.
- Not yet: changing the current directory (F4, since it is process-wide state), symbolic links
  to make, and permissions to set.

### Metadata

Rationale: [ADR-0096](../adr/0096-streams-files-and-drop.md), Q659.

| Function or method | Result |
|---|---|
| `metadata(path) -> Result<Metadata, io::Error>` | What `path` names, following a symbolic link |
| `symlink_metadata(path) -> Result<Metadata, io::Error>` | The same, of a symbolic link itself |
| `File::metadata()` | The same, of an open file |
| `remove_dir_all(path)` | Removes a directory with everything in it; a symbolic link is removed itself, not what it names; a path naming a file is an error |
| `canonicalize(path) -> Result<String, io::Error>` | The absolute path, with `.`, `..` and symbolic links resolved; on Windows it starts with `\\?\`, as Rust's |

`Metadata` has `len() -> u64`, `is_file()`, `is_dir()`, `is_symlink()` (which only
`symlink_metadata` can see), `readonly()` (no one may write it: Rust's
`permissions().readonly()`), and `modified()`, `accessed()` and `created()`, each a
`Result<SystemTime, io::Error>`: Linux and Android keep no creation time, and Intel Macs none in
the `stat` they use, so `created()` there is an error of kind `Unsupported`.

### `File` and `OpenOptions`

Rationale: [ADR-0096](../adr/0096-streams-files-and-drop.md), Q657. A `File` is an open file,
which reads and writes where its position is:

```sag
use std::fs::{File, OpenOptions};
use std::io::{Read, SeekFrom, Write};

const file = File::create("notes.txt")?;

file.write_str("hello world")?;
file.seek(SeekFrom::Start(6))?;

const log = OpenOptions::new().append(true).create(true).open("log.txt")?;
```

| Function or method | Result |
|---|---|
| `File::open(path)` | `Result<File, io::Error>`: the file, to read it |
| `File::create(path)` | The file, to write it, created or emptied |
| `OpenOptions::new()` | No option set; `read(b)`, `write(b)`, `append(b)`, `truncate(b)`, `create(b)` and `create_new(b)` each give the options with one set, and `open(path)` opens with them |
| `read`, `read_to_end`, `read_to_string`, `read_exact`, `write`, `write_all`, `write_str`, `flush` | Those of [`Read` and `Write`](#streams); `flush` does nothing, since a file has no buffer of its own |
| `seek(position: SeekFrom)` | `Result<u64, io::Error>`: the new position, from the start |
| `set_len(size: u64)` | Cuts the file to `size` bytes or adds zeros; the position stays |
| `sync_all()` | Waits until the system has stored what was written |
| `close()` | Closes the file now, reporting what the system says; closing it again does nothing |
| `unsafe File::from_raw_handle(handle: usize, path)` | The file of a descriptor (POSIX) or `HANDLE` (Windows) that nothing else will close, as Rust's `from_raw_fd` |
| `as_raw_handle() -> usize` | Its descriptor or `HANDLE`, which stays the file's, as Rust's `as_raw_fd`; after `close` it names nothing |
| `try_clone()` | `Result<File, io::Error>`: another `File` of the same open file, sharing its position, as Rust's ([ADR-0097](../adr/0097-processes-and-environment.md), Q664) |

- A `File` implements `Read`, `Write` and [`Drop`](04-classes.md#drop): it closes when its last
  reference goes, and an error there is lost, which `close()` reports. Reading, writing or seeking
  after `close` is an error (`Bad file descriptor`, of kind `Other`).
- As Rust's, `OpenOptions` asks for options that go together, or `open` is an error of kind
  `InvalidInput`: reading or writing, `truncate` with `write` and without `append`, and `create`
  or `create_new` with `write` or `append`. `append` writes every write at the end; `create_new`
  fails, of kind `AlreadyExists`, when the file exists; a file that does not exist without
  `create` is of kind `NotFound`.
- `OpenOptions` is a struct: each method gives a copy with one option set, so the calls chain as
  Rust's do.


## `std::hint`

`black_box<T>(value: T) -> T` returns its argument, but the compiler and the C compiler must
assume it was read and may have changed, so a benchmark keeps the work it measures:
`sum = sum.wrapping_add(hint::black_box(i) * 3)`. It takes any type. As Rust's, it is a best
effort, not a guarantee ([ADR-0090](../adr/0090-benchmarks.md)).

## `std::math`

```sag
use std::math;

fn main() {
  const angle = math::PI / 4.0;
  println(`${math::clamp(angle, 0.0, 0.5)} ${(-angle).abs().floor()} ${0.0.lerp(10.0, 0.25)}`);
}
```

| Constant (`f64`) | Value |
|---|---|
| `PI`, `TAU`, `E` | π, 2π, e |
| `SQRT_2`, `LN_2`, `LN_10` | √2, ln 2, ln 10 |
| `EPSILON` | the difference between `1.0` and the next larger `f64` |
| `MIN`, `MAX` | the smallest and largest finite `f64` |
| `INFINITY`, `NEG_INFINITY`, `NAN` | positive and negative infinity, not a number |

- For `f32`, convert them: `math::PI as f32`.
- The functions `abs`, `signum`, `copysign`, `min`, `max`, `clamp`, `lerp`, `floor`, `ceil`,
  `round`, `trunc`, `fract`, `is_nan`, `is_infinite`, `is_finite`, `sqrt`, `cbrt`, `powi`,
  `powf`, `exp`, `exp2`, `ln`, `log2`, `log10`, `log`, `hypot`, `mul_add`, `sin`, `cos`, `tan`,
  `asin`, `acos`, `atan`, `sinh`, `cosh`, `tanh`, `to_degrees` and `to_radians` take `f64`s
  (`powi` an `i32` exponent) and are the
  [methods of floats](02-types.md#methods-of-numbers-g1) of the same name: `math::lerp(a, b, t)`
  is `a.lerp(b, t)`. `math::atan2(y, x)` is `y.atan2(x)`, in C's order. A panic in `math::clamp`
  reports its place in the standard library.

## `std::mem`

```sag
use std::mem;

const size = mem::size_of<Vertex>();
var properties: PhysicalDeviceProperties = unsafe { mem::zeroed() };
```

| Function | Result |
|---|---|
| `size_of<T>() -> usize` | The size of `T` in bytes, as C's `sizeof` on the target |
| `align_of<T>() -> usize` | The alignment of `T` in bytes, as C's `_Alignof` |
| `zeroed<T>() -> T` | A value with every byte zero; needs `unsafe` |
| `replace<T>(place: &mut T, value: T) -> T` | Puts `value` in the place and gives what was there |
| `take<T: Default>(place: &mut T) -> T` | Gives what is in the place and leaves its [default](05-interfaces-generics.md#defaults) there |
| `swap<T>(a: &mut T, b: &mut T)` | Exchanges the values of two places |

- `size_of` and `align_of` take any type, written as a type argument.
- `zeroed` takes its type as a type argument or from the expected type. It only makes types valid
  with every byte zero: numbers, `bool`, raw pointers, `Option` of C function pointers, enums with
  a variant of value `0`, and `@repr(C)` structs, unions and arrays of them. Other types, such as
  a C function pointer (never null) or a struct without `@repr(C)`, are errors.

## `std::path`

Functions on paths as text; they never touch the file system. `/` separates components on
every system, and `\` too on Windows, where a path may also start with a drive (`C:`). The
root of a path is its leading separators, after the drive on Windows.

| Function | Result |
|---|---|
| `join(base, part) -> String` | `part` after `base` with a separator between them; `part` itself if it has a root or `base` is empty |
| `file_name(path) -> Option<String>` | The last component; `None` for `""`, a root, `.` or `..` |
| `parent(path) -> Option<String>` | The path without its last component; `Some("")` for a single relative component, `None` for `""` or a root |
| `extension(path) -> Option<String>` | The text after the last `.` of the file name; `None` without a `.` or when the name only starts with one (`.bashrc`) |
| `stem(path) -> Option<String>` | The file name without its extension |

- Trailing separators are ignored: `file_name("a/b/")` is `Some("b")`.
- `join` separates with `/`, or with `\` on Windows when `base` uses only `\`.

```sag
use std::path;

path::join("logs", "app.txt")         // "logs/app.txt"
path::file_name("logs/app.tar.gz")    // Some("app.tar.gz")
path::extension("logs/app.tar.gz")    // Some("gz")
path::stem("logs/app.tar.gz")         // Some("app.tar")
path::parent("logs/app.tar.gz")       // Some("logs")
```

## `std::process`

`exit(code: i32)` ends the program with the exit code `code`. It never returns, so like `panic`
it can stand where any value is expected (`const n = if ok { 1 } else { process::exit(2) };`).

- It writes out the buffered output, then ends the program without running pending releases:
  the leak check of the test harness does not report the objects still alive.
- Exit codes outside 0–255 depend on the operating system; POSIX systems keep the low 8 bits.

`abort()` ends the program at once, as Rust's: it never returns either, and writes out nothing,
so output still in a buffer may be lost ([ADR-0097](../adr/0097-processes-and-environment.md),
Q667). It is C's `abort`: on POSIX systems the signal `SIGABRT` ends the program, so its
`ExitStatus` has no code; on Windows, Microsoft's C runtime ends it with the status `0xC0000409`,
as Rust's does, and MinGW's with the exit code 3.

`id() -> u32` is the identifier the system gave this program, the one a `Child` started from it
has as its `id()`.

### Running programs

```sag
use std::process::Command;

const output = new Command("git").arg("status").arg("--short").current_dir("repo").output()?;
if output.status.success() {
  print(output.stdout);
}
```

`Command` is a class; each of these methods changes the command and returns it, so calls chain:

| Method | Effect |
|---|---|
| `new Command(program)` | The program to run, looked up in the directories of `PATH` unless it contains a separator (on Windows, also with `.exe`) |
| `arg(text)`, `args(list)` | Adds arguments, passed exactly as given |
| `env(name, value)` | Sets an environment variable for the program; the others are inherited |
| `current_dir(path)` | The program's working directory |
| `input(text)` | Text that `output` and `status` write to the program's stdin, which they then close; it replaces `stdin(...)` |
| `stdin(stdio)`, `stdout(stdio)`, `stderr(stdio)` | What a stream of the program is (a [`Stdio`](#streams-of-a-program)); `stdin` replaces `input(...)` |

| Method | Result |
|---|---|
| `output() -> Result<Output, io::Error>` | Runs the program to its end, capturing its stdout and stderr |
| `status() -> Result<ExitStatus, io::Error>` | Runs the program to its end, printing to this program's stdout and stderr |
| `spawn() -> Result<Child, io::Error>` | Starts the program, which [runs while this one goes on](#started-programs) |

Unless `stdin`, `stdout` or `stderr` set them, `output()` pipes stdout and stderr to capture them,
and `status()` and `spawn()` give the program this program's three streams; the stdin of
`output()` and `status()` is a pipe with `input(text)`, else this program's.

```sag
pub struct ExitStatus {
  pub code: Option<i32>;   // None if a signal ended the program (POSIX systems)
  pub fn success(self) -> bool;   // whether the code is 0
}

pub struct Output {
  pub status: ExitStatus;
  pub stdout: String;
  pub stderr: String;
}
```

- A program that ends with a code other than 0 is not an error: `success()` is `false`.
- A program that cannot be started is an error, such as
  `cannot run "tool": No such file or directory` of kind `NotFound`. An argument, variable or
  directory containing a NUL character, or a variable name that is empty or contains `=`, is an
  error of kind `InvalidInput`.
- Captured output that is not valid UTF-8 has each ill-formed sequence replaced by U+FFFD.
- This program's buffered output is written out before the child starts, so what both print
  appears in order.
- Not yet: removing or clearing inherited variables, and connecting one program's stdout to
  another's stdin.

### Streams of a program

Rationale: [ADR-0097](../adr/0097-processes-and-environment.md), Q664. A `Stdio`, a class, is what
a stream of a program to run is, as Rust's:

| Function | The stream |
|---|---|
| `Stdio::inherit()` | This program's |
| `Stdio::piped()` | A pipe to this program: for `spawn`, the `stdin`, `stdout` or `stderr` of the `Child` |
| `Stdio::null()` | Nothing: reading it gives the end at once, and what is written to it goes nowhere |
| `Stdio::from(file)`, `file.into()` | The `File` `file`, through a copy of its handle (`try_clone`), so that `file` stays this program's; it implements `From<File>` |

- A `Stdio::from(file)` of a closed file makes `spawn`, `output` and `status` fail with that
  error.

### Started programs

Rationale: [ADR-0097](../adr/0097-processes-and-environment.md), Q665 and Q666.

```sag
use std::io::BufReader;
use std::process::{Command, Stdio};

const child = new Command("sort").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;

{
  const input = child.stdin.take().unwrap();

  input.write_str("pear\napple\n")?;
}                                    // closed: `sort` sees the end of its input

for line in new BufReader(child.stdout.take().unwrap()).lines() {
  println(line?);                    // apple, then pear
}

println(child.wait()?.success());
```

A `Child` has the fields `pub var stdin: Option<ChildStdin>`, `stdout: Option<ChildStdout>` and
`stderr: Option<ChildStderr>`, `Some` for a stream set to `Stdio::piped()`, which
[`take()`](02-types.md#optiont-and-resultt-e-f1) takes out. `ChildStdin` implements `io::Write`,
the other two `io::Read`, and the three `Drop`, which closes the pipe.

| Method | Result |
|---|---|
| `id() -> u32` | The identifier the system gave the program |
| `wait() -> Result<ExitStatus, io::Error>` | Closes `stdin`, unless it was taken, then waits for the program to end |
| `try_wait() -> Result<Option<ExitStatus>, io::Error>` | How the program ended, or `None` while it runs |
| `kill() -> Result<(), io::Error>` | Ends the program at once: `SIGKILL`, or `TerminateProcess` with the exit code 1 on Windows; one that ended already is not an error |
| `wait_with_output() -> Result<Output, io::Error>` | Closes `stdin`, reads the piped `stdout` and `stderr` still in the `Child` to their end, both at once, and waits |

- A status found is kept: waiting again gives it, and `kill` after it does nothing.
- `spawn` with `input(text)` set is an error of kind `InvalidInput`: the stdin of a started
  program is written through its pipe.
- A write to a `ChildStdin` whose program closed its stdin is an error (`Broken pipe`, of kind
  `Other`), not a signal that ends this program.
- The `drop` of a `Child` closes its handle and the pipes still in it, and neither waits for the
  program nor kills it, as Rust's: the program goes on, and on POSIX systems it stays a zombie
  until this program ends unless it was waited for.

## `std::sync`

Rationale: [ADR-0046](../adr/0046-mutex-and-atomics.md),
[ADR-0047](../adr/0047-channels-and-thread-utilities.md),
[ADR-0098](../adr/0098-locks-orderings-and-threads.md).

```sag
use std::sync::{AtomicI64, Mutex, Ordering};
use std::thread;

const hits = new AtomicI64(0);
const names = new Mutex("");
const worker = thread::spawn(() => {
  hits.fetch_add(1, Ordering::Relaxed);
  names.update((text) => `${text}a`);
});

worker.join();
println(`${hits.load(Ordering::Relaxed)} ${names.get()}`);
```

A [static](03-memory.md#global-state-g1) holds a `Mutex` or an atomic that every thread
reaches without passing it: `static HITS: AtomicI64 = new AtomicI64(0);`.

| Item | Meaning |
|---|---|
| `Mutex<T: Send>` | A value that threads share, changed by one thread at a time |
| `new Mutex(value)`, `m.get() -> T`, `m.set(value)` | Creates it, copies the value out, replaces it |
| `m.update(f: fn(T) -> T) -> T` | Replaces the value with `f(value)` while no other thread can use the mutex, and returns the new value |
| `m.lock() -> MutexGuard<T>`, `m.try_lock()` | Holds the lock until the guard goes ([locks](#lock-guards-and-conditions)) |
| `Condvar`, `RwLock<T: Send>`, `Barrier` | A condition to wait on, a lock for many readers or one writer, a meeting point of threads |
| `AtomicBool`, `AtomicI8`, `AtomicI16`, `AtomicI32`, `AtomicI64`, `AtomicIsize`, `AtomicU8`, `AtomicU16`, `AtomicU32`, `AtomicU64`, `AtomicUsize` | A value of that type that threads read and change at once ([atomics](#atomics)) |
| `AtomicPtr<T>` | A `*mut T` that threads read and change at once |
| `Channel<T: Send>` | A queue of values that threads send to each other, without a limit |
| `new Channel<T>()`, `c.send(value)` | Creates it; adds `value` at the end. Sending on a closed channel panics |
| `c.receive() -> Option<T>` | The value at the front, waiting until there is one; `None` once the channel is closed and empty |
| `c.close()` | No more values can be sent; those sent are still received. Closing again does nothing |
| `c.iter() -> Iter<T>` | The values received one by one, until the channel is closed and empty |

- `Mutex<T>` and the atomics are `Send` (a `Mutex<T>` when `T` is), so threads can capture
  them. Since `T` must be `Send`, a mutex cannot hold a `List` or a class with `var` fields.
- `get`, `set` and `update` take and release the lock themselves. A thread locking a mutex it
  holds, through a guard or inside the function it gave to `update`, panics instead of waiting
  forever.
- Any thread holding a channel may send, receive or close it. Several receivers share the
  values: each value goes to one of them.

### Lock guards and conditions

Rationale: [ADR-0098](../adr/0098-locks-orderings-and-threads.md), Q671–Q674. As Rust's, without
poisoning: a panic ends the whole program, so no lock is left held by a thread that failed.

```sag
use std::sync::{Condvar, Mutex};

const queue = new Mutex(0);
const changed = new Condvar();
const worker = thread::spawn(() => {
  const guard = queue.lock();            // held until the end of this scope

  guard.set(guard.get() + 1);
  changed.notify_one();
});
const guard = changed.wait_while(queue.lock(), (count) => count == 0);
```

| Item | Meaning |
|---|---|
| `m.lock() -> MutexGuard<T>` | Waits for the lock of the `Mutex` and holds it until the guard goes |
| `m.try_lock() -> Result<MutexGuard<T>, TryLockError>` | The guard if no thread holds the lock, this one included; else `Err(TryLockError::WouldBlock)` |
| `guard.get() -> T`, `guard.set(value)` | A copy of the value; replaces it |
| `new Condvar()` | A condition that threads holding a `MutexGuard` wait on |
| `c.wait(guard) -> MutexGuard<T>` | Releases the lock until the condition is notified, then takes it again; it may also return without a notification |
| `c.wait_while(guard, condition: fn(T) -> bool) -> MutexGuard<T>` | Waits while `condition` holds for the value |
| `c.wait_timeout(guard, duration)`, `c.wait_timeout_while(guard, duration, condition)` | The same for at most `duration`: `(MutexGuard<T>, WaitTimeoutResult)`, whose `timed_out()` says whether the time ran out (while `condition` still held) |
| `c.notify_one()`, `c.notify_all()` | Wakes one, or every, thread waiting on it |
| `new RwLock(value)` | A value that many threads read at once, or one writes |
| `r.read() -> RwLockReadGuard<T>`, `r.write() -> RwLockWriteGuard<T>` | Holds the lock to read (`get`) or to write (`get`, `set`) until the guard goes |
| `r.try_read()`, `r.try_write()` | The guard, or `Err(TryLockError::WouldBlock)` if another thread writes, or reads or writes |
| `r.get()`, `r.set(value)`, `r.update(f)` | As `Mutex`'s; `get` reads while others read too |
| `new Barrier(count)`, `b.wait() -> BarrierWaitResult` | Waits until `count` threads have called it (0 is as 1), then lets them go together; `is_leader()` is true for one of them. It can be used again |

- A guard is released when its last reference goes: a guard in a local at the end of its scope, a
  temporary at the end of its statement, as in Rust. So `m.lock().get() + m.get()` locks the
  mutex twice in one statement, and panics. A guard kept in a field or a list holds the lock until
  it goes.
- The guards are not `Send` (`E0330`, `E0332`): the thread that took a lock releases it. The
  mutex, the condition, the `RwLock` and the barrier are, and threads share them.
- A `Condvar` waits with one mutex: waiting with another panics
  (`attempted to use a condition variable with two mutexes`), as Rust's may.
- A thread writing an `RwLock` that takes it again panics; one reading it that reads again may
  wait forever if a writer waits in between, as Rust's says.
- `MutexGuard::new`, `RwLockReadGuard::new`, `RwLockWriteGuard::new`, `WaitTimeoutResult::new` and
  `BarrierWaitResult::new` take types of `std::sys`, which programs cannot name: only the lock or
  the condition makes them.

### Atomics

Rationale: [ADR-0087](../adr/0087-atomic-storage-and-std-sync.md),
[ADR-0098](../adr/0098-locks-orderings-and-threads.md), Q676 and Q677. Every operation of an
atomic takes how it orders the memory operations around it, an `Ordering`, as Rust's
`std::sync::atomic`:

| `Ordering` | Meaning |
|---|---|
| `Relaxed` | Only the operation itself is atomic |
| `Release` | The writes before it are seen by a thread that reads its value with `Acquire` |
| `Acquire` | It sees the writes made before a `Release` whose value it reads |
| `AcqRel` | Both, for an operation that reads and writes |
| `SeqCst` | `AcqRel`, and every thread sees all `SeqCst` operations in one order |

| Method | Result |
|---|---|
| `new AtomicI64(value)` | The atomic |
| `load(order) -> T`, `store(value, order)` | Reads the value, writes it |
| `swap(value, order) -> T` | Writes `value` and returns the previous value |
| `compare_exchange(current, desired, success, failure) -> Result<T, T>` | Writes `desired` if the value is `current`: `Ok(current)` if it did, `Err(actual)` if not; `success` orders the exchange, `failure` the read of a value that differs |
| `compare_exchange_weak(current, desired, success, failure)` | The same, but it may fail although the value is `current`; faster in a loop on some processors |
| `fetch_update(set_order, fetch_order, f: fn(T) -> Option<T>) -> Result<T, T>` | Writes what `f` returns for the value while it returns `Some`, trying again if another thread changed it: `Ok(previous)`, or `Err(previous)` once `f` gives `None` |
| `fetch_add`, `fetch_sub`, `fetch_and`, `fetch_or`, `fetch_xor`, `fetch_nand`, `fetch_max`, `fetch_min` `(value, order) -> T` | Integers: writes the sum, difference, `&`, `\|`, `^`, `!(a & b)`, larger or smaller of the value and `value`, and returns the previous value; the sum and difference wrap around at the type's limits, also in debug builds |
| `fetch_and`, `fetch_or`, `fetch_xor`, `fetch_nand` `(value, order)`, `fetch_not(order)` | `AtomicBool`: the same with `&&`, `\|\|`, `!=` and `!(a && b)`, and the opposite |

`AtomicPtr<T>` has `load`, `store`, `swap`, `compare_exchange`, `compare_exchange_weak` and
`fetch_update`; what its pointer points to is read and written through `unsafe`, as any raw
pointer's ([FFI](10-ffi.md)). `sync::fence(order)` orders the memory operations around it
without an atomic of its own, as Rust's `atomic::fence`.

```sag
use std::sync::{AtomicBool, AtomicI64, Ordering};
use std::thread;

const data = new AtomicI64(0);
const ready = new AtomicBool(false);
const reader = thread::spawn(() => {
  while !ready.load(Ordering::Acquire) {}

  data.load(Ordering::Relaxed)               // 42: `Acquire` sees what came before `Release`
});

data.store(42, Ordering::Relaxed);
ready.store(true, Ordering::Release);
```

- The orderings Rust rejects panic, as Rust's: `load` with `Release` or `AcqRel`
  (`there is no such thing as a release load`), `store` with `Acquire` or `AcqRel`, a `failure`
  ordering of `Release` or `AcqRel`, and a `Relaxed` fence. A `failure` ordering stronger than
  `success` is allowed, as in Rust.
- `Ordering` is written `sync::Ordering`, or imported with `use std::sync::Ordering`, which hides
  the prelude's `Ordering` of comparisons in that file; a mismatch between the two notes that
  they are different types with the same name.
- The atomics are `Send`, and a [static](03-memory.md#global-state-g1) can hold one.

```sag
const jobs: Channel<i64> = new Channel();
const worker = thread::spawn(() => {
  for job in jobs.iter() {
    println(`job ${job}`);
  }
});

jobs.send(1);
jobs.close();
worker.join();
```

## `std::thread`

Rationale: [ADR-0045](../adr/0045-threads-and-send.md).

```sag
use std::thread;

const name = "Ada";
const handle = thread::spawn(() => `hello, ${name}`);
println(handle.join());
```

| Item | Meaning |
|---|---|
| `spawn(body: fn() -> T) -> JoinHandle<T>` | Runs `body` in a new thread |
| `JoinHandle<T>` | A thread started by `spawn` |
| `handle.join() -> T` | Waits for the thread to finish and returns what `body` returned; again, the same value |
| `sleep(duration: Duration)` | Pauses the running thread for at least `duration`; the same as `time::sleep` |
| `available_parallelism() -> usize` | The number of logical processors, at least 1 |

- The argument of `spawn` is a lambda or the name of a function, not another function value.
  Every binding the lambda captures must be `Send`
  ([interfaces](05-interfaces-generics.md#send-f4)); the compiler reports each one that is not,
  with the reason. `spawn` cannot be used as a value.
- What `body` returns needs not be `Send`: everything the thread creates is reachable only from
  it, and passes whole to the thread that calls `join`. A thread can build and return a `List`.
- A panic in any thread ends the whole program with exit code 101, as in the main thread.
- A thread that is never joined keeps running. When `main` returns, the program ends without
  waiting for the threads still running; the leak check of the test harness only runs when none
  is.
- `JoinHandle::spawn(body)` is the same as `thread::spawn(body)`, which calls it.
  `new JoinHandle<T>()` makes a handle without a thread, whose `join` panics.

## `std::time`

Rationale: [ADR-0039](../adr/0039-time-and-commands.md).

| Item | Meaning |
|---|---|
| `Duration` | A span of time with nanosecond precision, up to about 584 years |
| `Instant` | A point of a monotonic clock, which never goes back; for measuring |
| `SystemTime` | A point of the system clock, which the user or the network may change |
| `sleep(duration: Duration)` | Pauses the program for at least `duration` |

| Function | Result |
|---|---|
| `Duration::from_secs(n)`, `from_millis(n)`, `from_micros(n)`, `from_nanos(n)` | A `Duration` from a `u64` |
| `d.as_secs()`, `as_millis()`, `as_micros()`, `as_nanos()` | The whole units, as `u64` |
| `d.as_secs_f64()` | The seconds with their fraction |
| `d.plus(e)`, `d.minus(e)`, `d.times(n: u64)` | Arithmetic; `minus` panics if `e` is longer, also in release builds |
| `Instant::now()`, `i.elapsed()` | The current instant, and the time since `i` |
| `i.duration_since(earlier)` | The time from `earlier` to `i`; zero if `earlier` is later |
| `SystemTime::now()`, `t.since_unix_epoch()` | The current time, and the time since 1970-01-01 00:00:00 UTC (zero before it) |
| `UNIX_EPOCH`, `t + d` | 1970-01-01 00:00:00 UTC, a `static`, and a `SystemTime` moved later by a `Duration`, as Rust's; `SystemTime::default()` is the epoch ([ADR-0096](../adr/0096-streams-files-and-drop.md), Q659) |

- The three types implement `Ord` (and so `Eq`): `a.compare(b)`, and lists of them can be
  sorted. Their arithmetic and comparisons are methods.
- Creating or adding durations beyond the range of `u64` nanoseconds follows integer overflow
  rules: a panic in debug builds.
- Calendar dates, time zones and formatting are not part of `std::time`.

## Implementation rules

The modules of `std`, except `std::ffi`, are written in Sagitario and embedded in the compiler.
They follow the rules of every module, with four exceptions that only the standard library has:

- It may declare functions of the runtime in `extern "C"` blocks: names starting with `sag_`,
  which may also take and return `String` and take `fn()` values (borrowed arguments, new
  references as results).
- It may name the prelude module as `std::prelude`, to implement the prelude's interfaces with
  types of the same name (`io::Error` implements `Error`).
- It may use the `prv` members of the prelude's types: `std::task` implements `sleep` and
  `block_on` on those of `Task` ([ADR-0072](../adr/0072-async-await-increments.md)).
- It may use C symbols that programs cannot: the reserved names of the system's C library
  (`__errno_location`), and the runtime's own names starting with `sag_` for its
  `@export` functions. The runtime's core calls these functions, such as
  `sag_time_monotonic` in `std::time`, so every program compiles their modules, and they are
  generated although no Sagitario code calls them. They are not exported by a library a program
  builds ([ADR-0085](../adr/0085-conditional-compilation-and-std-time.md), Q552).

`std` itself has a file of private helpers, which its modules call as `super::name`: like any
module's private items, only its submodules see them.

`std::sys` holds the C API of each system that the standard library is written on, and the code
that differs by system, chosen with [`@cfg`](09-modules.md#conditional-compilation-s1): its
functions, constants and structs, and the reading of `errno`. Only the modules of `std` can name
it; for a program it does not exist (`E0200`)
([ADR-0085](../adr/0085-conditional-compilation-and-std-time.md), Q551).

`std::sys::Atomic<T>`, provided by the compiler, holds a `bool` or an integer that threads read
and change at once; the atomics of `std::sync` are written on it. It is only the type of a
`const` field of a class, set once in the class's initializer with `Atomic::new(value)` and then
used through its methods: `load`, `store`, `swap`, `compare_exchange` and, on integers,
`fetch_add` and `fetch_sub`, all sequentially consistent and wrapping around at the type's
limits. Another `T` is an error (`E0318`), and so is any other place or use (`E0339`). Its C is an
`_Atomic` field, and a program holding one is compiled as one using threads
([ADR-0087](../adr/0087-atomic-storage-and-std-sync.md)).

`std::process::exit` is provided by the compiler, since no function written in Sagitario can
declare that it never returns. The compiler also checks the arguments of `thread::spawn`,
`JoinHandle::spawn` and `task::spawn_blocking` where code outside the standard library calls them.
