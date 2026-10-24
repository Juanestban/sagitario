/* Sagitario runtime: support code linked into every program. Strict, portable C11.
 * Defining SAG_RELEASE selects release semantics: integer overflow wraps instead of panicking. */
#ifndef SAGITARIO_RUNTIME_H
#define SAGITARIO_RUNTIME_H

#include <float.h>
#include <limits.h>
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

/* Like `sag_panic_at`, with a message held in a `String` (`expect`). */
_Noreturn void sag_panic_str_at(sag_string message, const char *file, int line);
/* Prints the error returned by `main` (`error: ...`) to stderr. */
void sag_report_main_error(sag_string message);

/* ---- String API (docs/spec/02-types.md#string-f1) -------------------------------------
 * Byte indices; strings are always valid UTF-8. Functions returning `sag_string` return it
 * with one reference owned by the caller. */

static inline size_t sag_string_len(sag_string s) {
  return s.len;
}
static inline bool sag_string_is_empty(sag_string s) {
  return s.len == 0;
}
sag_string sag_string_concat(sag_string a, sag_string b);
/* Byte-wise lexicographic order: negative, zero or positive. */
int sag_string_cmp(sag_string a, sag_string b);
bool sag_string_contains(sag_string s, sag_string pattern);
bool sag_string_starts_with(sag_string s, sag_string prefix);
bool sag_string_ends_with(sag_string s, sag_string suffix);
/* The byte index of the first occurrence, or SIZE_MAX. */
size_t sag_string_find(sag_string s, sag_string pattern);
sag_string sag_string_trim(sag_string s);
sag_string sag_string_trim_start(sag_string s);
sag_string sag_string_trim_end(sag_string s);
sag_string sag_string_to_ascii_uppercase(sag_string s);
sag_string sag_string_to_ascii_lowercase(sag_string s);
sag_string sag_string_replace(sag_string s, sag_string from, sag_string to);
sag_string sag_string_repeat_at(sag_string s, size_t count, const char *file, int line);
sag_string sag_string_substring_at(sag_string s, size_t start, size_t end, const char *file, int line);
/* Decodes the character at `*index` and advances `*index` past it. */
uint32_t sag_utf8_next(sag_string s, size_t *index);

/* Parsing: on success the value is stored and `*error` is the empty string; otherwise
 * `*error` holds Rust's message (owned by the caller). */
bool sag_parse_signed(sag_string s, int64_t min, int64_t max, int64_t *value, sag_string *error);
bool sag_parse_unsigned(sag_string s, uint64_t max, uint64_t *value, sag_string *error);
bool sag_parse_float(sag_string s, double *value, sag_string *error);

/* ---- char (ASCII) ------------------------------------------------------------------------ */

static inline bool sag_char_is_ascii_digit(uint32_t c) {
  return c >= '0' && c <= '9';
}
static inline bool sag_char_is_ascii_uppercase(uint32_t c) {
  return c >= 'A' && c <= 'Z';
}
static inline bool sag_char_is_ascii_lowercase(uint32_t c) {
  return c >= 'a' && c <= 'z';
}
static inline bool sag_char_is_ascii_alphabetic(uint32_t c) {
  return sag_char_is_ascii_uppercase(c) || sag_char_is_ascii_lowercase(c);
}
static inline bool sag_char_is_ascii_alphanumeric(uint32_t c) {
  return sag_char_is_ascii_alphabetic(c) || sag_char_is_ascii_digit(c);
}
/* Rust's definition: space, tab, line feed, form feed, carriage return. */
static inline bool sag_char_is_ascii_whitespace(uint32_t c) {
  return c == ' ' || c == '\t' || c == '\n' || c == '\f' || c == '\r';
}
static inline uint32_t sag_char_to_ascii_uppercase(uint32_t c) {
  return sag_char_is_ascii_lowercase(c) ? c - 32 : c;
}
static inline uint32_t sag_char_to_ascii_lowercase(uint32_t c) {
  return sag_char_is_ascii_uppercase(c) ? c + 32 : c;
}
/* The digit's value in `radix` (2 to 36), or UINT32_MAX; panics on an invalid radix. */
uint32_t sag_char_to_digit_at(uint32_t c, uint32_t radix, const char *file, int line);

/* ---- StringBuilder (ARC reference) -------------------------------------------------------- */

typedef struct sag_builder_obj *sag_builder;

sag_builder sag_builder_new(void);
void sag_builder_retain(sag_builder builder);
void sag_builder_release(sag_builder builder);
void sag_builder_append(sag_builder builder, sag_string s);
void sag_builder_append_char(sag_builder builder, uint32_t c);
size_t sag_builder_len(sag_builder builder);
bool sag_builder_is_empty(sag_builder builder);
void sag_builder_clear(sag_builder builder);
sag_string sag_builder_to_string(sag_builder builder);

/* ---- Class instances (ARC, docs/spec/03-memory.md) ---------------------------------------- */

/* An entry of a vtable: a method, converted back to its own type before it is called. */
typedef void (*sag_method)(void);

/* The header every class instance starts with. `strong` counts the references to the object;
 * when it reaches zero, `drop` releases the object's fields. The memory is freed once no weak
 * reference remains either. `vtable` lists the methods called with dynamic dispatch, or is NULL
 * when the class has none. */
typedef struct sag_object {
  size_t strong;
  size_t weak;
  void (*drop)(struct sag_object *object);
  const sag_method *vtable;
} sag_object;

/* Allocates `size` bytes for an object; panics when out of memory. */
void *sag_object_alloc(size_t size);
/* Gives a new object one reference, its drop function and its vtable. */
void sag_object_init(sag_object *object, void (*drop)(sag_object *object), const sag_method *vtable);
/* Both take a pointer to an object (whose first member is its `sag_object`), or NULL. */
void sag_object_retain(void *object);
/* Objects whose last reference goes away while another is being destroyed are destroyed after
 * it, in a loop, so long chains of objects do not exhaust the stack. */
void sag_object_release(void *object);
/* Weak references: they keep the memory of an object, not the object, alive. */
void sag_object_weak_release(sag_object *object);
/* Makes `*slot` refer weakly to `object` (or NULL), dropping its previous weak reference. */
void sag_object_weak_set(sag_object **slot, sag_object *object);
/* A new strong reference to `object` while it is alive, or NULL. */
sag_object *sag_object_upgrade(sag_object *object);

/* ---- Interface values (docs/spec/05-interfaces-generics.md) ------------------------------ */

/* The entries of an interface for one type: thunks calling its methods, default methods and
 * field getters and setters, and the itables of the interface's parents for the same type. */
typedef struct sag_itable {
  const struct sag_itable *const *parents;
  const sag_method *methods;
} sag_itable;

/* A value of an interface type: a reference to an object (an instance, or a box holding a
 * struct or enum) with its itable. */
typedef struct sag_dyn {
  sag_object *object;
  const sag_itable *itable;
} sag_dyn;

static inline void sag_dyn_retain(sag_dyn value) {
  sag_object_retain(value.object);
}
static inline void sag_dyn_release(sag_dyn value) {
  sag_object_release(value.object);
}

/* ---- Function values (docs/spec/08-closures.md) ----------------------------------------- */

/* A function or closure: the function, called with `env` first, and its environment (an ARC
 * object holding the captures), or NULL. */
typedef struct sag_fnval {
  sag_method fn;
  sag_object *env;
} sag_fnval;

static inline void sag_fnval_retain(sag_fnval value) {
  sag_object_retain(value.env);
}
static inline void sag_fnval_release(sag_fnval value) {
  sag_object_release(value.env);
}

/* ---- FFI (docs/spec/10-ffi.md) ----------------------------------------------------------- */

/* C's `char`, the type `std::ffi::c_char` stands for. */
#if CHAR_MIN < 0
typedef int8_t sag_c_char;
#else
typedef uint8_t sag_c_char;
#endif

/* A NUL-terminated copy of a string (`CString`), reference counted. */
typedef struct sag_cstring_obj *sag_cstring;

/* Copies `text` into `*out` and returns true, or returns false with Rust's message in `*error`
 * when the text contains a NUL byte. Both outputs are always written (NULL or ""). */
bool sag_cstring_new(sag_string text, sag_cstring *out, sag_string *error);
void sag_cstring_retain(sag_cstring c);
void sag_cstring_release(sag_cstring c);
const sag_c_char *sag_cstring_ptr(sag_cstring c);
/* Copies a NUL-terminated C string, replacing invalid UTF-8 with U+FFFD; panics on NULL. */
sag_string sag_string_from_c_at(const sag_c_char *p, const char *file, int line);
_Noreturn void sag_null_pointer_at(const char *file, int line);

static inline const uint8_t *sag_string_ptr(sag_string s) {
  return s.ptr;
}

/* Raw pointer operations; offsets and dereferences only happen inside `unsafe`. A null pointer
 * is checked before every dereference. */
#define SAG_PTR_IS_NULL(p) ((p) == NULL)
#define SAG_PTR_ADD(p, n) ((p) + (n))
#define SAG_ARRAY_PTR(a) ((a).data)
#define SAG_ARRAY_MUT_PTR(a) ((a).data)
#define SAG_CHECK_PTR(p, file, line) ((p) != NULL ? (p) : (sag_null_pointer_at(file, line), (p)))

/* ---- Arrays ------------------------------------------------------------------------------ */

_Noreturn void sag_index_out_of_bounds(size_t index, size_t len, const char *file, int line);

/* Returns `index` if it is below `len`; panics otherwise, in every build mode. */
static inline size_t sag_index_at(size_t index, size_t len, const char *file, int line) {
  if (index >= len) {
    sag_index_out_of_bounds(index, len, file, line);
  }
  return index;
}

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

/* ---- Lists (docs/spec/02-types.md#arrays-and-lists) -------------------------------------- */

/* A `List<T>`: an object holding `len` elements of `size` bytes in `data`, which has room for
 * `cap` elements plus one, where `sag_list_remove` leaves the removed element. `release`
 * releases one element, or is NULL when elements hold no references. */
typedef struct sag_list {
  sag_object header;
  size_t len;
  size_t cap;
  size_t size;
  unsigned char *data;
  void (*release)(void *element);
} sag_list;

sag_object *sag_list_new(size_t size, void (*release)(void *element));
/* A list with a copy of `len` elements, whose references the list takes over. */
sag_object *sag_list_from(size_t size, void (*release)(void *element), const void *elements, size_t len);

static inline size_t sag_list_len(sag_object *list) {
  return ((sag_list *)list)->len;
}

/* The element at `index`; panics if it is out of bounds. */
static inline void *sag_list_at(sag_object *list, size_t index, const char *file, int line) {
  sag_list *self = (sag_list *)list;
  return self->data + sag_index_at(index, self->len, file, line) * self->size;
}

/* The slot of a new last element, which the caller fills. */
void *sag_list_push(sag_object *list);
/* The slot of a new element at `index` (after moving the following ones), which the caller
 * fills; panics if `index` is past the end. */
void *sag_list_insert(sag_object *list, size_t index, const char *file, int line);
/* Removes the last element, which must exist, and returns where it is until the list changes. */
void *sag_list_pop(sag_object *list);
/* Removes the element at `index` and returns where it is until the list changes; panics if it
 * is out of bounds. */
void *sag_list_remove(sag_object *list, size_t index, const char *file, int line);
void sag_list_clear(sag_object *list);
void sag_list_reverse(sag_object *list);

/* ---- Hashing and order (docs/spec/05-interfaces-generics.md) ------------------------------ */

uint64_t sag_hash_u64(uint64_t value);
uint64_t sag_string_hash(sag_string text);
/* -1, 0 or 1 by the IEEE 754 total order: -NaN < -inf < ... < -0 < +0 < ... < inf < NaN. */
int32_t sag_f32_total_compare(float a, float b);
int32_t sag_f64_total_compare(double a, double b);

/* The integer at an offset computed modulo 2^N by `for` loops over ranges with steps; like the
 * arithmetic above, converting to a signed type keeps the low bits. */
#define SAG_FROM_UINTMAX(NAME, T)                         \
  static inline T sag_from_uintmax_##NAME(uintmax_t value) { \
    return (T)value;                                        \
  }

SAG_FROM_UINTMAX(i8, int8_t)
SAG_FROM_UINTMAX(i16, int16_t)
SAG_FROM_UINTMAX(i32, int32_t)
SAG_FROM_UINTMAX(i64, int64_t)
SAG_FROM_UINTMAX(isize, intptr_t)
SAG_FROM_UINTMAX(u8, uint8_t)
SAG_FROM_UINTMAX(u16, uint16_t)
SAG_FROM_UINTMAX(u32, uint32_t)
SAG_FROM_UINTMAX(u64, uint64_t)
SAG_FROM_UINTMAX(usize, size_t)

/* ---- Standard library: std::env, std::io, std::process --------------------------------- *
 * Declared in Sagitario by the standard library modules (docs/spec/11-standard-library.md).
 * They borrow their string arguments and return new references. */

/* Codes of I/O errors: `errno` values are positive, the runtime's own are negative. */
#define SAG_IO_INVALID_DATA (-2)
#define SAG_IO_UNKNOWN (-3)
/* The status of a read at the end of the input. */
#define SAG_IO_END (-1)
/* Indices of the variants of `std::io::ErrorKind`. */
#define SAG_IO_KIND_NOT_FOUND 0
#define SAG_IO_KIND_PERMISSION_DENIED 1
#define SAG_IO_KIND_ALREADY_EXISTS 2
#define SAG_IO_KIND_INVALID_INPUT 3
#define SAG_IO_KIND_INVALID_DATA 4
#define SAG_IO_KIND_OTHER 6

size_t sag_env_arg_count(void);
/* Arguments and variables that are not valid UTF-8 get each bad sequence replaced by U+FFFD. */
sag_string sag_env_arg(size_t index);
sag_string sag_env_var(sag_string name, bool *found);
/* Writes to stdout (stream 1) or stderr (2); returns 0 or an error code. */
int32_t sag_io_write(int32_t stream, sag_string text, bool newline);
int32_t sag_io_flush(void);
/* `*status` becomes 0, SAG_IO_END or an error code. */
sag_string sag_io_read_line(int32_t *status);
sag_string sag_io_read_all(int32_t *status);
int32_t sag_io_error_kind(int32_t code);
sag_string sag_io_error_message(int32_t code);
/* Flushes output and ends the program without running pending releases. */
_Noreturn void sag_process_exit(int32_t code);

/* What a path names, for std::fs; errors count as missing. */
#define SAG_FS_MISSING 0
#define SAG_FS_FILE 1
#define SAG_FS_DIR 2
#define SAG_FS_OTHER 3

/* Each returns 0 or an error code, or sets `*status` to one. Paths with NUL are EINVAL. */
int32_t sag_fs_kind(sag_string path);
sag_string sag_fs_read(sag_string path, int32_t *status);
int32_t sag_fs_write(sag_string path, sag_string text, bool append);
int32_t sag_fs_create_dir(sag_string path);
int32_t sag_fs_remove_dir(sag_string path);
int32_t sag_fs_remove_file(sag_string path);
/* Replaces the target if it exists, on every system. */
int32_t sag_fs_rename(sag_string from, sag_string to);
int32_t sag_fs_copy(sag_string from, sag_string to);
/* The names in a directory, without `.` and `..`, separated by NUL characters. */
sag_string sag_fs_read_dir(sag_string path, int32_t *status);
sag_string sag_fs_current_dir(int32_t *status);
/* Whether paths follow Windows rules (`\` separators and drive letters). */
bool sag_path_windows(void);

/* ---- Program lifecycle ----------------------------------------------------------------- */

void sag_rt_init(int argc, char **argv);
/* Flushes output and returns the exit code of a normal exit. If the environment variable
 * SAG_LEAK_CHECK is set and ARC objects are still alive, reports them and returns 102. */
int sag_rt_finish(void);

#endif
