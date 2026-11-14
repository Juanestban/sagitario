# 0097. Processes and the environment (S2)

- Status: Accepted
- Date: 2026-11-12
- Refines: [ADR-0084](0084-a-complete-language-and-the-engine-apart.md) (Q542: record 8 of the
  audit of S2), [ADR-0093](0093-everyday-methods.md) (Q619: `take` and `replace` of `Option`
  waited for a way to change the place that holds it), docs/spec/11-standard-library.md
  (`std::process`, `std::env`)

## Context

Record 8 of the audit of S2 ([`docs/std-audit.md`](../std-audit.md)) is the rest of Rust's
`process` and `env`. Sagitario's `Command` runs a program to its end (`output`, `status`) with
its arguments, variables, directory and an input text; it cannot start one and go on (`spawn`),
talk to it through pipes, wait for it or kill it, nor send its streams to files. `std::process`
has `exit`, and `std::env` reads arguments and variables.

A started program is a `Child` whose pipes, in Rust, are public `Option` fields taken out with
`child.stdin.take()`, so that dropping the pipe closes it while the `Child` stays. Sagitario has
no `Option::take`: [ADR-0093](0093-everyday-methods.md) (Q619) left it out because it changes the
place holding the option, which a method of an enum cannot do, and `mem::take(&mut child.stdin)`
cannot lend a field holding references through a class instance
([ADR-0079](0079-references-and-borrowing.md), Q491).

## Decision

| # | Topic | Decision |
|---|---|---|
| Q663 | `take` and `replace` of `Option` | `place.take()` gives the value of an `Option` place and leaves `None` in it; `place.replace(value)` gives it and leaves `Some(value)`; as Rust's. The compiler writes them, as it writes `a[i] += v`: the receiver must be a place an assignment could change, under the same rules (`E0312` for a `const` binding or a `const` field, `E0307` for a value that is not a place), and is evaluated once. A field of a class instance qualifies when it is `var`, so `child.stdin.take()` works. This replaces what Q619 said stood for them. |
| Q664 | `Stdio` | A class of `std::process` saying what a stream of a started program is, as Rust's: `Stdio::inherit()` (this program's), `Stdio::piped()` (a pipe to this program), `Stdio::null()` (nothing), and `Stdio::from(file)` (a `File`, a duplicate of its handle, so the `File` stays this program's). `Command` gains `stdin(stdio)`, `stdout(stdio)` and `stderr(stdio)`. Unless they are set, `output()` pipes stdout and stderr to capture them, and `status()` and `spawn()` inherit the three. `input(text)` stays, for `output()` and `status()`: it pipes stdin and writes `text` to it; it and `stdin(...)` replace each other. |
| Q665 | `spawn` and `Child` | `Command::spawn() -> Result<Child, io::Error>` starts the program and goes on. A `Child` has `pub var stdin: Option<ChildStdin>`, `pub var stdout: Option<ChildStdout>` and `pub var stderr: Option<ChildStderr>`, `Some` for a stream set to `Stdio::piped()`; `ChildStdin` implements `Write`, the other two `Read`, and all three `Drop`, which closes the pipe. Its methods: `id() -> u32`, `wait() -> Result<ExitStatus, io::Error>` (closing `stdin` first, as Rust's, so the program sees the end of its input), `try_wait() -> Result<Option<ExitStatus>, io::Error>`, `kill() -> Result<(), io::Error>` (`SIGKILL`, or `TerminateProcess` on Windows; one that ended already is not an error), and `wait_with_output() -> Result<Output, io::Error>`, which reads its piped stdout and stderr to their end and waits. A status found is kept: waiting again gives it. `spawn` with `input(text)` set is an error of kind `InvalidInput`: a started program's stdin is written through its pipe. |
| Q666 | The end of a `Child` | Its `drop` closes its pipes and its handle, and neither waits for the program nor kills it, as Rust's: the program goes on, and on POSIX systems it stays a zombie until this program ends, unless it was waited for. |
| Q667 | `std::process` | `abort()`, which ends the program at once without the work of an exit and never returns, as `exit`, which the compiler provides; and `id() -> u32`, the identifier of this program. |
| Q668 | `std::env` | `vars() -> List<(String, String)>` (a list, as `args()` is), `unsafe set_var(name, value)` and `unsafe remove_var(name)`, `unsafe` as in Rust 2024, since another thread, or C code, may read the environment at the same time; a name that is empty or holds `=` or a NUL panics, as Rust's. `set_current_dir(path) -> Result<(), io::Error>`, safe, as in Rust. `current_exe() -> Result<String, io::Error>`, `temp_dir() -> String` and `home_dir() -> Option<String>`. The constants `OS` (`"linux"`, `"macos"`, `"windows"`, `"android"`, `"ios"`), `ARCH` (`"x86_64"`, `"aarch64"`, `"arm"`) and `FAMILY` (`"unix"`, `"windows"`), of the target, as Rust's `std::env::consts`, are written `env::OS`, since a module of `std` is one file. |
| Q669 | The editor | `sag lsp` completes the new types and functions as any of `std`, and `take` and `replace` on an `Option`; the README lists them. |
| Q670 | Increments | **1** `take` and `replace` of `Option` (Q663). **2** `Stdio`, `spawn`, `Child` and its pipes (Q664–Q666). **3** `std::process` and `std::env` (Q667, Q668). Each one with its tests, the spec and the editor, and the rows of the audit marked. |

### Example

```sag
use std::io;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn main() -> Result<(), io::Error> {
  const child = new Command("sort").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;

  {
    const input = child.stdin.take().unwrap();

    input.write_str("pear\napple\n")?;
  }                                                    // closed: `sort` sees the end

  for line in new BufReader(child.stdout.take().unwrap()).lines() {
    println(line?);                                    // apple, then pear
  }

  println(f"{child.wait()?.success()} {env::OS}");
  Ok(())
}
```

## Alternatives considered

- **Methods that take the pipes out** (`child.take_stdin()`): no change to the language, but
  Rust's form, which was chosen, is the field and `take`.
- **Safe `set_var` with a lock of `std::env`:** it would protect Sagitario code, not C code
  calling `getenv`, which is why Rust 2024 made them `unsafe`.
- **A `Child` that waits for, or kills, its program on `drop`:** no zombies, or no stray
  programs, but a `Child` let go would hang this program or stop the other by surprise.
- **Named methods instead of `Stdio`** (`pipe_stdout()`, `stdout_to(file)`): no new type, but
  many methods and a departure from Rust.

## Consequences

- `Option` gains `take` and `replace` on places, for any program, not only for pipes.
- Programs can be started, fed, read and waited for while this program goes on, and their
  streams sent to files or to nothing.
- The audit's rows of `Command`, `process::exit, abort, id`, `env::vars...` and `env::consts` are
  done.

## Notes

- Increment 1 is done. `take` and `replace` are methods the compiler writes
  (`BuiltinMethod::Take` and `Replace`): the checker checks the receiver as the target of an
  assignment, the flow analysis counts it as one for the warning of an unneeded `var`, and
  lowering reads the place into a value of its own before storing `None` or `Some(value)`, the
  argument first.
- Increment 2 is done. `std::sys` starts a program with each stream inherited, a pipe, the null
  device or a file (`start_process`), and waits for it, polls it, kills it and releases it; the
  pipes are `sys::Pipe`s closed once. `output` and `status` are now built on that start, and
  `exchange`, which writes the input and reads both outputs at once, serves them and
  `wait_with_output`. On Windows this program's ends of the pipes stay overlapped, so a read or a
  write through `ChildStdout` or `ChildStdin` waits for its own operation; on POSIX systems a write
  ignores `SIGPIPE`, and a Linux child closes a file or null device it was given once it has
  them as its streams. Besides Q664 and Q665: `File::try_clone`, Rust's, which makes the copy of
  `Stdio::from`, and `File::as_raw_handle`, Rust's `as_raw_fd`, the counterpart of
  `from_raw_handle`; `Stdio::from` is Rust's `From<File>`, so `file.into()` works too. The
  initializers of `Child` and of its pipes, and `Stdio::system`, which gives `Command` the stream,
  are public, as `Metadata::from_system` is, since a class cannot reach another's private
  members, but they take or give types of `std::sys`, which programs cannot name.
- Increment 3 is done, and with it the ADR. `abort` is a function the compiler provides, as
  `exit` (`StdItem::Abort`), lowered to the runtime's `sag_process_abort`, which is C's `abort`:
  with Microsoft's C runtime it ends the program with `0xC0000409`, as Rust's `__fastfail` does,
  and with MinGW's with 3. `std::sys` reads the environment as `Command` already did (`environ`,
  `GetEnvironmentStringsW`), and changes it with `setenv` and `unsetenv`, or
  `SetEnvironmentVariableW`, which is what `Command` and `variable` read, so a variable set is
  seen by both. `home_dir` follows Rust on POSIX systems, `HOME` and then `getpwuid_r`, whose
  `pw_dir` is read at its offset on each system; on Windows it reads `USERPROFILE` only, since
  Rust's fallback, `GetUserProfileDirectoryW`, is in `userenv.dll`, which programs do not link.
  `temp_dir` on Windows is `GetTempPathW`, not the `GetTempPath2W` Rust prefers where it exists,
  which only differs for programs running as `SYSTEM`. `sag lsp` now completes `exit` and `abort`
  after `std::process::`, which it did not offer for `exit` before.
