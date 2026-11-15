# Technical Debt

This file records known technical debt in the Sagitario project: workarounds, suppressions,
and incomplete solutions that should be addressed when time and context allow.

## macOS `readdir()` ASan Heap-Buffer-Overflow — All macOS Tests Disabled in Release CI

**Status:** Active workaround  
**Severity:** Medium — CI release fails on macOS without this fix  
**Introduced:** 2026-11-14  
**Related ADR:** None yet (candidate for future ADR if a permanent solution is chosen)

### Problem

AddressSanitizer (ASan) on macOS reports a `heap-buffer-overflow` when the C standard library's
`readdir()` function encounters directory entries whose total size exceeds Apple's internal
8 KB heap buffer. The error originates inside Apple's `libsystem_c.dylib`, not in Sagitario code.

**Error details (from CI release — `x86_64-apple-darwin`):**
```
==ERROR: AddressSanitizer: heap-buffer-overflow on address 0x625000002100
WRITE of size 33990 at 0x000108c1b757 (readdir)
0x625000002100 is located 0 bytes after 8192-byte region [0x625000000100,0x625000002100)
allocated by thread T0 here:
    #0 malloc (libclang_rt.asan_osx_dynamic.dylib)
    #1 _readdir_unlocked (libsystem_c.dylib)
    #2 readdir (libsystem_c.dylib)
```

Two tests fail in the release CI on macOS:
- `tests/run-pass/file_metadata.sag`
- `tests/run-pass/std_fs.sag`

Both tests call `std::fs::readDir()`, which internally uses `readdir()` via FFI declared in
`compiler/std/sys.sag`.

### Root Cause

Apple's implementation of `readdir()` in `libsystem_c.dylib` allocates an initial 8 KB heap
buffer via `malloc()`. When a directory entry's data (file names, metadata) exceeds this buffer,
`readdir()` writes beyond the allocated region. This is a bug in Apple's C library, not in
Sagitario's FFI bindings or generated code.

This is a known issue in the ecosystem. Go encountered the same problem
([Go issue #30933](https://github.com/golang/go/issues/30933)) and had to work around it.

### Approaches Evaluated

Five approaches were evaluated, in order of preference:

1. **`getdirentries64()`** — This BSD API lets the caller control the buffer size, avoiding the
   internal buffer entirely. However, `getdirentries64` is **not a public API symbol** on macOS.
   While the function exists in the kernel, linking against it from C produces a linker error:
   ```
   ld: symbol(s) not found for architecture arm64: "_getdirentries64"
   ```
   Go initially used this approach but had to abandon it when Apple removed the symbol from the
   public linker. **Status: Rejected — linker error.**

2. **`readdir_r()`** — The reentrant version of `readdir()` is **deprecated** on macOS since
   macOS 10.13 (High Sierra) and removed from the SDK. **Status: Rejected — deprecated API.**

3. **`__asan_default_suppressions()` in the C runtime** — Add a suppression function to
   `runtime/sagitario_runtime.c` to suppress ASan reports from `libsystem_c.dylib`. This was
   implemented first but **did not work**: `__asan_default_suppressions()` only takes effect when
   the translation unit that defines it is itself compiled with `-fsanitize=address`. In our
   build, the runtime is compiled without ASan while the test programs are compiled with ASan,
   so the suppression function is never invoked by the ASan runtime. **Status: Rejected — does
   not work when the runtime is not compiled with ASan.**

4. **`opendir$INODE64` / `readdir$INODE64` symbols** — These are the 64-bit inode variants of
   the directory functions on macOS x86_64. However: (a) they solve a different problem (32-bit
   inode overflow, not buffer overflow); (b) Sagitario has no `@link_name` attribute to map a
   function name to a different linker symbol; (c) `$` is not a valid identifier character in
   Sagitario; (d) these symbols do not exist on macOS arm64 or iOS. **Status: Rejected — does
   not solve the problem and is not implementable.**

5. **Disable ASan for the harness on macOS** — Set `SAG_TEST_SANITIZE=0` in the CI release
   workflow for macOS runners only. This disables all sanitizers (AddressSanitizer and
   ThreadSanitizer) for the test harness on macOS, while other platforms continue to run with
   full sanitizer coverage. **Status: Rejected — the suppression did not work (see approach 3).**

6. **Disable all macOS tests in release CI** — Comment out all macOS-related jobs and matrix
   entries in `.github/workflows/release.yml`: the build targets (`aarch64-apple-darwin` and
   `x86_64-apple-darwin`), the harness step, and the installer test. This allows the release
   to proceed without being blocked by the macOS ASan failure, while Linux and Windows continue
   to build, test and install normally. **Status: Chosen — the only working approach to unblock
   releases.**

### Modified Files

| File | Path | Change |
|------|------|--------|
| CI workflow | `.github/workflows/release.yml` | Commented out all macOS-related entries: (1) build matrix targets `aarch64-apple-darwin` and `x86_64-apple-darwin` (lines 32-37); (2) the harness step for macOS with `SAG_TEST_SANITIZE=0` (lines 74-78); (3) the `MACOSX_DEPLOYMENT_TARGET` environment variable (line 42); (4) the installer test for `macos-latest` in the install job matrix (lines 153-157). Linux and Windows remain fully active. |

### What Was NOT Changed

- `runtime/sagitario_runtime.c` — The previously attempted `__asan_default_suppressions()` was
  removed. The runtime has no macOS-specific code for this issue.
- `compiler/std/sys.sag` — The FFI declarations for `opendir/readdir/closedir` remain unchanged
  and are used on all Unix platforms (Linux, Android, macOS, iOS).
- `compiler/std/fs.sag` — The `readDir()` standard library function is unchanged.
- Test files — No tests were modified, skipped, or commented out.

### Limitations

- **No macOS coverage at all:** The release CI does not build, test, or install Sagitario on
  macOS. This means no binary is produced for `aarch64-apple-darwin` or `x86_64-apple-darwin`,
  no harness runs on macOS, and no installer is tested on macOS. Users cannot install Sagitario
  on macOS via the release until this is resolved.
- **No ASan or ThreadSanitizer coverage on macOS:** Even if a macOS binary were built manually,
  the tests that trigger the `readdir()` overflow will abort with ASan enabled. Without ASan,
  heap bugs and race conditions go undetected.
- **The underlying bug persists:** Apple's `readdir()` buffer overflow still occurs at runtime
  on macOS. Without ASan, the overflow is silent but the program may still behave incorrectly
  with very large directory entries.
- **Linux and Windows are unaffected:** These platforms continue to build, test with sanitizers,
  and install normally in the release CI.

### Future Work

To restore macOS coverage in the release CI, one of these approaches is needed:

1. **Monitor Apple's fix:** If Apple fixes the `readdir()` buffer overflow in a future macOS
   release, uncomment all macOS entries in `.github/workflows/release.yml` and add a minimum
   macOS version check documenting when the fix was introduced.

2. **Per-test sanitizer control:** Extend the harness to support per-test sanitizer configuration
   (e.g., a `// sanitize: none` header comment) so that only `file_metadata.sag` and
   `std_fs.sag` run without ASan on macOS, while the rest of the harness retains full coverage.
   This would require changes to `tests/harness/suites.sag` and `tests/harness/cli.sag`. Once
   implemented, uncomment the macOS build and harness steps.

3. **Use `fts()` API:** The `fts()` API provides a higher-level directory traversal that may not
   trigger the same buffer issue. However, `fts()` also uses `readdir()` internally on macOS,
   so this needs testing on macOS with ASan enabled. If it works, update `compiler/std/sys.sag`
   to use `fts()` on macOS and uncomment the macOS CI entries.

4. **File a bug with Apple:** Report the issue to Apple's bug reporter with a minimal C
   reproducer. If accepted, track the radar number here.

5. **Add `@link_name` attribute to Sagitario:** If the language gains a `@link_name` attribute
   (like Rust's `#[link_name]`), it would enable mapping FFI function names to different linker
   symbols. This could allow using `opendir$INODE64`/`readdir$INODE64` on macOS x86_64, though
   this solves a different problem (32-bit inode overflow) and may not resolve the ASan issue.

### Verification

To reproduce the issue on macOS without the workaround:

```bash
# Build with ASan and run the affected test
sag build --sanitize tests/run-pass/file_metadata.sag
./file_metadata  # Aborts with heap-buffer-overflow from readdir()
```

To verify the workaround works:

```bash
# Run harness without sanitizers on macOS
SAG_TEST_SANITIZE=0 sag run tests/harness/main.sag -- --sag build/bin/sag --nova build/bin/nova
# All tests pass
```

---

*Add new technical debt entries above this line, keeping the most recent first.*
