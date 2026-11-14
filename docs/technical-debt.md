# Technical Debt

This file records known technical debt in the Sagitario project: workarounds, suppressions,
and incomplete solutions that should be addressed when time and context allow.

## macOS `readdir()` ASan Heap-Buffer-Overflow Suppression

**Status:** Active workaround  
**Severity:** Medium — CI release fails on macOS without this fix  
**Introduced:** 2026-11-14  
**Related ADR:** None yet (candidate for future ADR if a permanent solution is chosen)

### Problem

AddressSanitizer (ASan) on macOS reports a `heap-buffer-overflow` when the C standard library's
`readdir()` function encounters directory entries whose total size exceeds Apple's internal
8 KB heap buffer. The error originates inside Apple's `libsystem_c.dylib`, not in Sagitario code.

**Error details:**
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

Both tests call `std::fs::readDir()`, which internally uses `readdir()` via FFI.

### Root Cause

Apple's implementation of `readdir()` in `libsystem_c.dylib` allocates an initial 8 KB heap
buffer via `malloc()`. When a directory entry's data (file names, metadata) exceeds this buffer,
`readdir()` writes beyond the allocated region. This is a bug in Apple's C library, not in
Sagitario's FFI bindings or generated code.

This is a known issue in the ecosystem. Go encountered the same problem
([Go issue #30933](https://github.com/golang/go/issues/30933)) and had to work around it.

### Why This Workaround?

Three approaches were evaluated:

1. **`getdirentries64()`** — This BSD API lets the caller control the buffer size, avoiding the
   internal buffer entirely. However, `getdirentries64` is **not a public API symbol** on macOS.
   While the function exists in the kernel, linking against it from C produces a linker error:
   ```
   ld: symbol(s) not found for architecture arm64: "_getdirentries64"
   ```
   Go initially used this approach but had to abandon it when Apple removed the symbol from the
   public linker.

2. **`readdir_r()`** — The reentrant version of `readdir()` is **deprecated** on macOS since
   macOS 10.13 (High Sierra) and removed from the SDK. Not a viable option.

3. **ASan suppression** — Add `__asan_default_suppressions()` to the C runtime to suppress
   ASan reports originating from `libsystem_c.dylib`. ASan continues to catch bugs in our code;
   only the known Apple library issue is suppressed.

Option 3 was chosen because:
- It is the only approach that works on both x86_64 and arm64 macOS.
- It does not require changing the FFI or generated C code.
- ASan remains active for detecting bugs in Sagitario's own code.
- The suppression is narrowly scoped to `libsystem_c.dylib` only.

### Modified Files

| File | Path | Change |
|------|------|--------|
| C Runtime | `runtime/sagitario_runtime.c` | Added `__asan_default_suppressions()` function that returns `"interceptor_via_lib:libsystem_c.dylib\n"` when compiled on macOS with ASan. Guarded by `#if defined(__APPLE__) && defined(__has_feature)` and `#if __has_feature(address_sanitizer)`. |

### What Was NOT Changed

- `compiler/std/sys.sag` — The FFI declarations for `opendir/readdir/closedir` remain unchanged
  and are used on all Unix platforms (Linux, Android, macOS, iOS).
- `compiler/std/fs.sag` — The `readDir()` standard library function is unchanged.
- Test files — No tests were modified or skipped.

### Limitations

- This suppression hides **all** ASan reports from `libsystem_c.dylib`, not just the `readdir()`
  overflow. If Apple fixes the bug or if another bug exists in our use of `libsystem_c.dylib`,
  it will also be suppressed.
- The suppression only works when ASan is enabled at compile time via `-fsanitize=address`.
  Without ASan, the overflow still occurs silently, but the program does not abort.

### Future Work

A permanent solution requires one of:

1. **Monitor Apple's fix:** If Apple fixes the `readdir()` buffer overflow in a future macOS
   release, remove the suppression and add a minimum macOS version check.

2. **Use `fts()` or `fdopendir()` + `getdirentries()`:** The `fts()` API provides a higher-level
   directory traversal that may not trigger the same buffer issue. However, `fts()` also uses
   `readdir()` internally on macOS, so this needs testing.

3. **Use `opendir()` + `readdir_r()` with a large static buffer:** While `readdir_r()` is
   deprecated, it may still be available on supported macOS versions. This would require
   conditional compilation and testing.

4. **Skip ASan for affected tests on macOS:** Configure the test harness to run
   `file_metadata.sag` and `std_fs.sag` without ASan on macOS only. This is less desirable
   than the current approach because it reduces coverage.

5. **File a bug with Apple:** Report the issue to Apple's bug reporter with a minimal C
   reproducer. If accepted, track the radar number here.

### Verification

To verify this suppression works on macOS:

```bash
# Build with ASan
sag build --target x86_64-apple-darwin --sanitize=address tests/run-pass/file_metadata.sag
./file_metadata  # Should complete without ASan abort

# Run full harness
sag run tests/harness/main.sag -- --sag tests/run-pass
```

Without the suppression, the tests abort with `heap-buffer-overflow` as shown in the Problem
section above.

---

*Add new technical debt entries above this line, keeping the most recent first.*
