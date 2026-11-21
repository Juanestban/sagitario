# Technical Debt

This file records known technical debt in the Sagitario project: workarounds, suppressions,
and incomplete solutions that should be addressed when time and context allow.

No known technical debt. The last entry, the macOS `readdir()` failure under AddressSanitizer
that turned the macOS release off, was fixed by
[ADR-0108](adr/0108-link-names.md): `std::sys` called the symbols of 32-bit inodes on macOS for
Intel, which ASan's interceptor reads as the 64-bit ones.

---

*Add new technical debt entries above this line, keeping the most recent first.*
