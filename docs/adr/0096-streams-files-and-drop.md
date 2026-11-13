# 0096. Streams, files and `Drop` (S2)

- Status: Accepted
- Date: 2026-11-12
- Refines: [ADR-0084](0084-a-complete-language-and-the-engine-apart.md) (Q542: record 7 of the
  audit of S2), [ADR-0027](0027-classes-and-arc.md) (user destructors were deferred),
  docs/spec/03-memory.md and docs/spec/04-classes.md ("there are no destructors in the MVP"),
  docs/spec/11-standard-library.md (`std::io`, `std::fs`)

## Context

Record 7 of the audit of S2 ([`docs/std-audit.md`](../std-audit.md)) is Rust's streams and
files: `Read`, `Write`, `BufRead`, `BufReader`, `BufWriter`, `File` with `OpenOptions` and
`seek`, `fs::metadata`, `remove_dir_all`, `canonicalize`, and the bytes of stdin and stdout.
Sagitario's `std::fs` reads and writes whole files, and `std::io` writes text and reads lines.

A `File` holds a descriptor of the system, which must be closed. Rust closes it in `Drop`, when
the value goes out of scope. Sagitario has no destructors: [ADR-0027](0027-classes-and-arc.md)
deferred them, because a destructor could keep `self` alive after its object is freed. But ARC
already frees each class instance deterministically, when its last reference goes, through a
`drop` function the compiler writes for each class; a destructor is a call at its start.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q655 | `Drop` | A prelude interface `Drop { fn drop(self); }`, as Swift's `deinit`: for classes only, since structs and enums are copied and have no last copy (implementing it on one is an error, `E0329`). When the last strong reference to an instance goes, the runtime calls its `drop` once, then releases its fields. `drop` is a method of the class, virtual as the others: a subclass that implements it calls `zod.drop()` to run its parent's. If, when `drop` returns, `self` has a strong reference again (stored in a field, a list or a static), the program panics (`drop kept a reference to its instance`), which is the resurrection ADR-0027 feared. Calling `drop` directly is an error (`E0314`), as in Rust: the last reference going calls it. Instances in a cycle that `weak` does not break are never freed, so their `drop` never runs; `exit()` and a panic end the process without running it, as Rust's. |
| Q656 | `Read` and `Write` | Interfaces of `std::io`, as Rust's, on slices ([ADR-0079](0079-references-and-borrowing.md)): `Read { fn read(self, buffer: &mut [u8]) -> Result<usize, Error>; }`, with the default methods `read_to_end() -> Result<List<u8>, Error>`, `read_to_string() -> Result<String, Error>` (invalid UTF-8 is an error of kind `InvalidData`) and `read_exact(buffer: &mut [u8]) -> Result<(), Error>` (an end before is `UnexpectedEof`); `Write { fn write(self, bytes: &[u8]) -> Result<usize, Error>; fn flush(self) -> Result<(), Error>; }`, with `write_all(bytes: &[u8])` and `write_str(text: String)`, which stands for Rust's `write!`. `read` gives `0` at the end. The functions of `std::io` there are today stay. |
| Q657 | `File` | A class of `std::fs` implementing `Read`, `Write` and `Drop`, which closes it. `File::open(path)` reads, `File::create(path)` writes and truncates, and `OpenOptions` chooses as Rust's: `OpenOptions::new().append(true).create(true).open(path)`, with `read`, `write`, `append`, `truncate`, `create` and `create_new`. A `File` has `seek(position: SeekFrom) -> Result<u64, Error>`, with the enum `SeekFrom { Start(u64), End(i64), Current(i64) }`, `metadata()`, `set_len(size: u64)`, `sync_all()`, and `close() -> Result<(), Error>`, which closes it now and reports what the system says; after `close`, reading or writing is an error, and `drop` does nothing. |
| Q658 | Buffers | `BufRead`, an interface of `std::io` extending `Read`, with `read_line() -> Result<Option<String>, Error>` (the next line without its `\n` or `\r\n`, `None` at the end, as `io::read_line`) and the default `lines() -> Iter<Result<String, Error>>`; Rust's `fill_buf` lends a buffer, which Sagitario cannot. `BufReader<R: Read>` implements `Read` and `BufRead` over any reader, and `BufWriter<W: Write>` implements `Write` and `Drop`, which flushes it and, as Rust's, ignores an error there (`flush()` reports it). Both have `new(inner)`, `with_capacity(n, inner)` and `get_ref()`. |
| Q659 | Metadata and `fs` | `fs::metadata(path) -> Result<Metadata, Error>` and `File::metadata()`, with a struct `Metadata` of `len() -> u64`, `is_file()`, `is_dir()`, `is_symlink()`, `readonly()` (Rust's `permissions().readonly()`), and `modified()`, `accessed()` and `created()`, each a `Result<SystemTime, Error>` (an error of kind `Unsupported` where the system has none). `fs::remove_dir_all(path)` and `fs::canonicalize(path) -> Result<String, Error>`. Symbolic links, permissions to set and the other functions of Rust's `std::os` wait for a record of their own. |
| Q660 | Standard streams | `io::stdin()`, `io::stdout()` and `io::stderr()` give handles: `Stdin` implements `Read` and `BufRead`, `Stdout` and `Stderr` implement `Write`, so bytes go through them. `Stdout::lock()` gives a `StdoutLock`, which implements `Write` and holds the lock of stdout until its `drop`, so a series of writes from one thread is not interleaved with other threads' `print`, which takes the same lock. |
| Q661 | The editor | `Drop` and the new types are completed by `sag lsp` as any type of the prelude and of `std`; snippets for `impl Drop`, `impl Read` and `impl Write`; the README lists them. |
| Q662 | Increments | **1** `Drop` (Q655): the runtime's call, the checks of the checker and the panic on resurrection. **2** `Read`, `Write`, `File`, `OpenOptions` and `SeekFrom` (Q656, Q657). **3** `BufRead`, `BufReader`, `BufWriter` and the standard streams (Q658, Q660). **4** Metadata, `remove_dir_all` and `canonicalize` (Q659). Each one with its tests, the spec and the editor, and the rows of the audit marked. |

### Example

```sag
use std::fs::{File, OpenOptions};
use std::io;
use std::io::{BufRead, BufReader, Write};

class Connection impl Drop {
  pub const name: String;

  pub fn new(self, name: String) { self.name = name; }
  pub fn drop(self) { println(f"closing {self.name}"); }
}

fn main() -> Result<(), io::Error> {
  const log = OpenOptions::new().append(true).create(true).open("log.txt")?;

  log.write_str("started\n")?;

  for line in BufReader::new(File::open("input.txt")?).lines() {
    println(line?);
  }

  {
    const connection = new Connection("db");
  }                                          // closing db

  Ok(())
}                                            // `log` is closed here, by its `drop`
```

## Alternatives considered

- **Only an explicit `close()`:** no change to the language, but a forgotten `close` keeps the
  descriptor until the process ends, and every type holding a resource repeats the rule.
- **A block bound to a scope (`defer`, `using`):** the end of the resource is visible in the
  code, but it is new syntax, and it cannot follow a resource stored in a field, which ARC can.
- **`Drop` on structs too:** Rust's `Drop` is on moved values; a struct in Sagitario is copied,
  so every copy would run it.
- **`read` and `write` by value** (`read(max) -> List<u8>`): simpler to call, but each read
  makes a list, where a slice reuses the buffer, and it departs from Rust.
- **A mode enum instead of `OpenOptions`:** shorter, but it cannot combine options.

## Consequences

- Classes can hold resources that free themselves; `File`, `BufWriter` and `StdoutLock` are the
  first, and sockets (record 10) and the guards of locks (record 9) can follow.
- The runtime's release of an instance calls a function of the program, which may panic.
- Code generic over streams (`fn copy<R: Read, W: Write>(from: R, to: W)`) reaches files,
  buffers, the standard streams and, later, sockets and processes.
- The audit's rows `Read, Write, BufRead...`, `File`, `fs::metadata...` and the missing bytes of
  `stdin, stdout` are done; `Drop` is no longer out of scope.

## Notes

- Increment 1 is done. Each instance of a class implementing `Drop` records the function that
  implements its `drop`, its own or inherited (`ClassInstance.user_drop`), and the C of the class
  gets a `sag_destroy_cN`, registered in place of `sag_drop_cN`, which calls it between
  `sag_object_drop_begin` and `sag_object_drop_end` of the runtime. These give the object one
  strong reference while `drop` runs, so copies of `self` do not destroy it again, and destroy at
  once what `drop` releases, by a loop of its own, so that a list or a closure holding `self` lets
  it go before the check of `drop_end`. The runtime's queue of objects to destroy, which was a
  stack, became first in, first out, for the order of Q655's spec: the elements of a list are
  dropped first to last, as Rust's.
- Increment 2 is done. A `File` holds a descriptor of the system as a `usize` (a `HANDLE` on
  Windows), so its fields are the same everywhere; `std::sys` opens, reads, writes, seeks, syncs,
  resizes and closes it. POSIX's `open` is variadic, which the FFI cannot declare, so files are
  opened with `fopen`, whose C11 modes (`x` for `create_new`) cover Rust's options but one: writing
  without truncating a file that may be new tries `r+`, then a new file; the stream's descriptor
  is duplicated with `dup` and the stream closed, and the `File` reads and writes the descriptor
  without stdio's buffer. Windows opens with `CreateFileW`, whose dispositions cover every option.
  Two departures from Q657, for want of module-private members: `OpenOptions` is a struct whose
  methods give copies, since a class cannot be made by `OpenOptions::new()`; and `OpenOptions`,
  another type, makes its `File` through `unsafe File::from_raw_handle`, as Rust's `from_raw_fd`,
  since the initializer of `File` is private to it.
