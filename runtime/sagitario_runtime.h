/* Sagitario runtime: support code linked into every program. Strict, portable C11.
 * Defining SAG_RELEASE selects release semantics: integer overflow wraps instead of panicking. */
#ifndef SAGITARIO_RUNTIME_H
#define SAGITARIO_RUNTIME_H

#include <float.h>
#include <math.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* ---- Panics ---------------------------------------------------------------------------- */

/* Prints `panic: message` to stderr and exits with code 101. */
_Noreturn void sag_panic(const char *message);
/* Prints `panic at file:line: message` to stderr and exits with code 101. */
_Noreturn void sag_panic_at(const char *message, const char *file, int line);

/* ---- Strings (ARC) --------------------------------------------------------------------- */

typedef struct sag_string_buf sag_string_buf;

/* An immutable UTF-8 string. `buf` owns the bytes and is reference counted; it is NULL for
 * static literals and the empty string, which are never freed. */
typedef struct sag_string {
  const uint8_t *ptr;
  size_t len;
  sag_string_buf *buf;
} sag_string;

sag_string sag_string_from_static(const char *bytes, size_t len);
sag_string sag_string_empty(void);
void sag_string_retain(sag_string s);
void sag_string_release(sag_string s);

/* Builds a new string; `sag_sb_finish` returns it with one reference. */
typedef struct sag_sb {
  uint8_t *data;
  size_t len;
  size_t cap;
} sag_sb;

void sag_sb_init(sag_sb *sb);
void sag_sb_push_bytes(sag_sb *sb, const char *bytes, size_t len);
void sag_sb_push_str(sag_sb *sb, sag_string s);
void sag_sb_push_i64(sag_sb *sb, int64_t value);
void sag_sb_push_u64(sag_sb *sb, uint64_t value);
/* Shortest decimal that converts back to the same value, without exponent. */
void sag_sb_push_f64(sag_sb *sb, double value);
void sag_sb_push_f32(sag_sb *sb, float value);
void sag_sb_push_bool(sag_sb *sb, bool value);
void sag_sb_push_char(sag_sb *sb, uint32_t c);
sag_string sag_sb_finish(sag_sb *sb);

/* ---- Printing (prelude) ---------------------------------------------------------------- */

void sag_print(sag_string s);
void sag_println(sag_string s);
void sag_eprintln(sag_string s);

/* ---- Integer arithmetic -----------------------------------------------------------------
 * Results are computed modulo 2^64 through uint64_t, so they never overflow in C; converting
 * back to a signed type keeps the low bits (two's complement on every supported target). The
 * checks only decide whether debug builds panic. */

static inline void sag_overflow(const char *message, const char *file, int line) {
#ifdef SAG_RELEASE
  (void)message;
  (void)file;
  (void)line;
#else
  sag_panic_at(message, file, line);
#endif
}

#define SAG_DIVISION_BY_ZERO(b, file, line, message) \
  if ((b) == 0) {                                     \
    sag_panic_at(message, file, line);                \
  }

#define SAG_SIGNED_OPS(NAME, T, MIN, MAX)                                                                 \
  static inline T sag_add_##NAME##_at(T a, T b, const char *file, int line) {                           \
    if ((b > 0 && a > (T)(MAX - b)) || (b < 0 && a < (T)(MIN - b))) {                                    \
      sag_overflow("attempt to add with overflow", file, line);                                         \
    }                                                                                                   \
    return (T)((uint64_t)a + (uint64_t)b);                                                              \
  }                                                                                                     \
  static inline T sag_sub_##NAME##_at(T a, T b, const char *file, int line) {                           \
    if ((b < 0 && a > (T)(MAX + b)) || (b > 0 && a < (T)(MIN + b))) {                                    \
      sag_overflow("attempt to subtract with overflow", file, line);                                    \
    }                                                                                                   \
    return (T)((uint64_t)a - (uint64_t)b);                                                              \
  }                                                                                                     \
  static inline T sag_mul_##NAME##_at(T a, T b, const char *file, int line) {                           \
    bool overflow = false;                                                                              \
    if (a > 0) {                                                                                        \
      overflow = b > 0 ? a > (T)(MAX / b) : b < (T)(MIN / a);                                           \
    } else if (a < 0) {                                                                                 \
      overflow = b > 0 ? a < (T)(MIN / b) : (b != 0 && b < (T)(MAX / a));                               \
    }                                                                                                   \
    if (overflow) {                                                                                     \
      sag_overflow("attempt to multiply with overflow", file, line);                                    \
    }                                                                                                   \
    return (T)((uint64_t)a * (uint64_t)b);                                                              \
  }                                                                                                     \
  static inline T sag_div_##NAME##_at(T a, T b, const char *file, int line) {                           \
    SAG_DIVISION_BY_ZERO(b, file, line, "attempt to divide by zero")                                    \
    if (a == MIN && b == -1) {                                                                          \
      sag_overflow("attempt to divide with overflow", file, line);                                      \
      return MIN;                                                                                       \
    }                                                                                                   \
    return (T)(a / b);                                                                                  \
  }                                                                                                     \
  static inline T sag_rem_##NAME##_at(T a, T b, const char *file, int line) {                           \
    SAG_DIVISION_BY_ZERO(b, file, line, "attempt to calculate the remainder with a divisor of zero")    \
    if (a == MIN && b == -1) {                                                                          \
      sag_overflow("attempt to calculate the remainder with overflow", file, line);                     \
      return 0;                                                                                         \
    }                                                                                                   \
    return (T)(a % b);                                                                                  \
  }                                                                                                     \
  static inline T sag_neg_##NAME##_at(T a, const char *file, int line) {                                \
    if (a == MIN) {                                                                                     \
      sag_overflow("attempt to negate with overflow", file, line);                                      \
      return MIN;                                                                                       \
    }                                                                                                   \
    return (T)(-a);                                                                                     \
  }

#define SAG_UNSIGNED_OPS(NAME, T, MAX)                                                                    \
  static inline T sag_add_##NAME##_at(T a, T b, const char *file, int line) {                           \
    if (a > (T)(MAX - b)) {                                                                             \
      sag_overflow("attempt to add with overflow", file, line);                                         \
    }                                                                                                   \
    return (T)((uint64_t)a + (uint64_t)b);                                                              \
  }                                                                                                     \
  static inline T sag_sub_##NAME##_at(T a, T b, const char *file, int line) {                           \
    if (a < b) {                                                                                        \
      sag_overflow("attempt to subtract with overflow", file, line);                                    \
    }                                                                                                   \
    return (T)((uint64_t)a - (uint64_t)b);                                                              \
  }                                                                                                     \
  static inline T sag_mul_##NAME##_at(T a, T b, const char *file, int line) {                           \
    if (b != 0 && a > (T)(MAX / b)) {                                                                   \
      sag_overflow("attempt to multiply with overflow", file, line);                                    \
    }                                                                                                   \
    return (T)((uint64_t)a * (uint64_t)b);                                                              \
  }                                                                                                     \
  static inline T sag_div_##NAME##_at(T a, T b, const char *file, int line) {                           \
    SAG_DIVISION_BY_ZERO(b, file, line, "attempt to divide by zero")                                    \
    return (T)(a / b);                                                                                  \
  }                                                                                                     \
  static inline T sag_rem_##NAME##_at(T a, T b, const char *file, int line) {                           \
    SAG_DIVISION_BY_ZERO(b, file, line, "attempt to calculate the remainder with a divisor of zero")    \
    return (T)(a % b);                                                                                  \
  }

SAG_SIGNED_OPS(i8, int8_t, INT8_MIN, INT8_MAX)
SAG_SIGNED_OPS(i16, int16_t, INT16_MIN, INT16_MAX)
SAG_SIGNED_OPS(i32, int32_t, INT32_MIN, INT32_MAX)
SAG_SIGNED_OPS(i64, int64_t, INT64_MIN, INT64_MAX)
SAG_SIGNED_OPS(isize, intptr_t, INTPTR_MIN, INTPTR_MAX)
SAG_UNSIGNED_OPS(u8, uint8_t, UINT8_MAX)
SAG_UNSIGNED_OPS(u16, uint16_t, UINT16_MAX)
SAG_UNSIGNED_OPS(u32, uint32_t, UINT32_MAX)
SAG_UNSIGNED_OPS(u64, uint64_t, UINT64_MAX)
SAG_UNSIGNED_OPS(usize, size_t, SIZE_MAX)

/* ---- Conversions (`as`) -----------------------------------------------------------------
 * Float to integer saturates and maps NaN to 0; LIMIT is 2^(bits-1) for signed types and 2^bits
 * for unsigned ones, both exact in a double. */

#define SAG_F2I_SIGNED(NAME, T, MIN, MAX, LIMIT) \
  static inline T sag_f2i_##NAME(double x) {     \
    if (x != x) {                                \
      return 0;                                  \
    }                                            \
    if (x <= -(LIMIT)) {                         \
      return MIN;                                \
    }                                            \
    if (x >= (LIMIT)) {                          \
      return MAX;                                \
    }                                            \
    return (T)x;                                 \
  }

#define SAG_F2I_UNSIGNED(NAME, T, MAX, LIMIT) \
  static inline T sag_f2i_##NAME(double x) {  \
    if (x != x || x <= -1.0) {                \
      return 0;                               \
    }                                         \
    if (x >= (LIMIT)) {                       \
      return MAX;                             \
    }                                         \
    return (T)x;                              \
  }

SAG_F2I_SIGNED(i8, int8_t, INT8_MIN, INT8_MAX, 128.0)
SAG_F2I_SIGNED(i16, int16_t, INT16_MIN, INT16_MAX, 32768.0)
SAG_F2I_SIGNED(i32, int32_t, INT32_MIN, INT32_MAX, 2147483648.0)
SAG_F2I_SIGNED(i64, int64_t, INT64_MIN, INT64_MAX, 9223372036854775808.0)
SAG_F2I_SIGNED(isize, intptr_t, INTPTR_MIN, INTPTR_MAX, 9223372036854775808.0)
SAG_F2I_UNSIGNED(u8, uint8_t, UINT8_MAX, 256.0)
SAG_F2I_UNSIGNED(u16, uint16_t, UINT16_MAX, 65536.0)
SAG_F2I_UNSIGNED(u32, uint32_t, UINT32_MAX, 4294967296.0)
SAG_F2I_UNSIGNED(u64, uint64_t, UINT64_MAX, 18446744073709551616.0)
SAG_F2I_UNSIGNED(usize, size_t, SIZE_MAX, 18446744073709551616.0)

/* Out-of-range values become infinity instead of being undefined behavior. */
static inline float sag_f64_to_f32(double x) {
  if (x > FLT_MAX) {
    return HUGE_VALF;
  }
  if (x < -FLT_MAX) {
    return -HUGE_VALF;
  }
  return (float)x;
}

/* ---- Program lifecycle ----------------------------------------------------------------- */

void sag_rt_init(int argc, char **argv);
/* Flushes output and returns the exit code of a normal exit. If the environment variable
 * SAG_LEAK_CHECK is set and ARC objects are still alive, reports them and returns 102. */
int sag_rt_finish(void);

#endif
