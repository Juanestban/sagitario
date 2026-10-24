# 11. Standard library (F3)

Rationale: [ADR-0037](../adr/0037-standard-library-env-io-process.md),
[ADR-0038](../adr/0038-file-system-and-paths.md), [ADR-0039](../adr/0039-time-and-commands.md).

The standard library is the set of modules under the path root `std`. A program imports them
with `use` like its own modules (`use std::io;`, `use std::env::args;`) or names them in full
(`std::process::exit(1)`). Only the modules a program names are compiled, and of those only the
functions it uses.

| Module | Contents | Milestone |
|---|---|---|
| `std::ffi` | C types, `CString`, `string_from_c` ([FFI](10-ffi.md)) | F1 |
| `std::env` | Command-line arguments and environment variables | F3 |
| `std::io` | Standard input and output with errors as values; `io::Error` | F3 |
| `std::fs` | Files and directories | F3 |
| `std::path` | Joining paths and taking them apart | F3 |
| `std::process` | `exit`, running programs | F3 |
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

## `std::env`

| Function | Result |
|---|---|
| `args() -> List<String>` | The command-line arguments, starting with the program as the operating system started it (usually its path) |
| `variable(name: String) -> Option<String>` | The value of an environment variable, or `None` if it is not set |

- Arguments and values that are not valid UTF-8 have each ill-formed sequence replaced by
  U+FFFD. On Windows they are read as UTF-16, so any Unicode text arrives intact.
- A variable name that is empty or contains `=` or a NUL character is never set: `variable`
  returns `None`.
- Arguments are not part of `main`'s signature ([errors](07-errors.md#program-entry-point-and-exit-codes)).

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

- The functions without a stated result return `Result<(), io::Error>`. The prelude's `print`,
  `println` and `eprintln` panic instead ([prelude](09-modules.md#prelude)); both write to the
  same streams, in order.
- Input that is not valid UTF-8 is an error of kind `InvalidData`; the bytes read are lost.

### `io::Error` and `io::ErrorKind`

```sag
pub enum ErrorKind {
  NotFound,
  PermissionDenied,
  AlreadyExists,
  InvalidInput,
  InvalidData,
  UnexpectedEof,
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

## `std::fs`

Rationale: [ADR-0038](../adr/0038-file-system-and-paths.md). Paths are `String`s, relative to the
current directory unless they have a root. The functions with a `Result` fail with an
`io::Error` whose message names the operation and the path:
`cannot read "notes.txt": No such file or directory`.

| Function | Result |
|---|---|
| `read_to_string(path) -> Result<String, io::Error>` | The whole file |
| `write(path, text)` | Creates the file, or replaces its contents |
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
  valid UTF-8 is an error of kind `InvalidData`.
- `exists`, `is_file` and `is_dir` return `false` when the path cannot be examined.
- Using a directory as a file, or a file as a directory, fails the same way on every system:
  messages such as "Is a directory" and "Not a directory", of kind `Other`.
- Names that are not valid UTF-8 are listed by `read_dir` with each ill-formed sequence replaced
  by U+FFFD. On Windows, paths are passed to the system as UTF-16, so any Unicode name works.
- A path containing a NUL character is an error of kind `InvalidInput`.
- Not yet: reading and writing bytes, file metadata (size, times, permissions), removing a
  directory tree, and changing the current directory (F4, since it is process-wide state).

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
| `input(text)` | Text written to the program's stdin, which is then closed; without it the program reads this program's stdin |

| Method | Result |
|---|---|
| `output() -> Result<Output, io::Error>` | Runs the program to its end, capturing its stdout and stderr |
| `status() -> Result<ExitStatus, io::Error>` | Runs the program to its end, printing to this program's stdout and stderr |

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
- Not yet: running a program in the background (`spawn`, `wait`, `kill`), redirecting to
  files, removing or clearing inherited variables, and connecting programs with pipes.

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

- The three types implement `Ord` (and so `Eq`): `a.compare(b)`, and lists of them can be
  sorted. Without operator overloading, arithmetic and comparisons are methods.
- Creating or adding durations beyond the range of `u64` nanoseconds follows integer overflow
  rules: a panic in debug builds.
- Calendar dates, time zones and formatting are not part of `std::time`.

## Implementation rules

The modules of `std`, except `std::ffi`, are written in Sagitario and embedded in the compiler.
They follow the rules of every module, with two exceptions that only the standard library has:

- It may declare functions of the runtime in `extern "C"` blocks: names starting with `sag_`,
  which may also take and return `String` (borrowed arguments, new references as results).
- It may name the prelude module as `std::prelude`, to implement the prelude's interfaces with
  types of the same name (`io::Error` implements `Error`).

`std` itself has a file of private helpers, which its modules call as `super::name`: like any
module's private items, only its submodules see them.

`std::process::exit` is provided by the compiler, since no function written in Sagitario can
declare that it never returns.
