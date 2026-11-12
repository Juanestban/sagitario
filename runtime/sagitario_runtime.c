/* `getenv` is standard C11; MSVC's CRT only deprecates it in favor of non-portable variants. */
#define _CRT_SECURE_NO_WARNINGS
/* The clocks and threads of the runtime are POSIX, not C11. */
#if !defined(_WIN32) && !defined(_POSIX_C_SOURCE)
#define _POSIX_C_SOURCE 200809L
#endif

#include "sagitario_runtime.h"

#include <errno.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef _WIN32
#include <sys/types.h>
#include <time.h>
#include <unistd.h>
#ifdef SAG_THREADS
#include <pthread.h>
#endif
#endif

/* ---- Reference counts and thread-local state ------------------------------------------- */

#ifdef SAG_THREADS
#define SAG_THREAD_LOCAL _Thread_local

static void sag_count_init(sag_count *count, size_t value) {
  atomic_init(count, value);
}
static void sag_count_inc(sag_count *count) {
  atomic_fetch_add_explicit(count, 1, memory_order_relaxed);
}
/* The new count. The release ordering publishes this thread's writes to the object; the acquire
 * ordering lets the thread that drops it see the writes of every other one. */
static size_t sag_count_dec(sag_count *count) {
  return atomic_fetch_sub_explicit(count, 1, memory_order_acq_rel) - 1;
}
static size_t sag_count_get(sag_count *count) {
  return atomic_load_explicit(count, memory_order_acquire);
}
/* Adds one unless the count is zero: a weak reference upgraded while another thread drops the
 * last strong one. */
static bool sag_count_inc_nonzero(sag_count *count) {
  size_t value = atomic_load_explicit(count, memory_order_relaxed);
  while (value != 0) {
    if (atomic_compare_exchange_weak_explicit(count, &value, value + 1, memory_order_acquire, memory_order_relaxed)) {
      return true;
    }
  }
  return false;
}
#else
#define SAG_THREAD_LOCAL

static void sag_count_init(sag_count *count, size_t value) {
  *count = value;
}
static void sag_count_inc(sag_count *count) {
  (*count)++;
}
static size_t sag_count_dec(sag_count *count) {
  return --*count;
}
static size_t sag_count_get(sag_count *count) {
  return *count;
}
static bool sag_count_inc_nonzero(sag_count *count) {
  if (*count == 0) {
    return false;
  }
  (*count)++;
  return true;
}
#endif

/* ---- Panics ---------------------------------------------------------------------------- */

/* Starts ending the program. With threads, the first one to end it does; any other blocks until
 * the program is gone, so that two threads never print a panic or call `exit` at once. */
static void sag_end_begin(void) {
#ifdef SAG_THREADS
  static atomic_flag ending = ATOMIC_FLAG_INIT;
  if (atomic_flag_test_and_set(&ending)) {
    for (;;) {
      sag_time_sleep(UINT64_C(1000000000));
    }
  }
#endif
  fflush(stdout);
}

_Noreturn void sag_panic(const char *message) {
  sag_end_begin();
  fprintf(stderr, "panic: %s\n", message);
  fflush(stderr);
  exit(101);
}

_Noreturn void sag_panic_at(const char *message, const char *file, int line) {
  sag_end_begin();
  fprintf(stderr, "panic at %s:%d: %s\n", file, line, message);
  fflush(stderr);
  exit(101);
}

_Noreturn void sag_index_out_of_bounds(size_t index, size_t len, const char *file, int line) {
  char message[96];
  snprintf(message, sizeof message, "index out of bounds: the len is %zu but the index is %zu", len, index);
  sag_panic_at(message, file, line);
}

_Noreturn void sag_panic_str_at(sag_string message, const char *file, int line) {
  sag_end_begin();
  fprintf(stderr, "panic at %s:%d: %.*s\n", file, line, (int)message.len, (const char *)message.ptr);
  fflush(stderr);
  exit(101);
}

/* ---- Strings (ARC) --------------------------------------------------------------------- */

struct sag_string_buf {
  sag_count refcount;
  size_t len;
  uint8_t bytes[];
};

/* Live ARC objects, checked at exit when SAG_LEAK_CHECK is set. */
static sag_count sag_live_objects;

sag_string sag_string_from_static(const char *bytes, size_t len) {
  sag_string s;
  s.ptr = (const uint8_t *)bytes;
  s.len = len;
  s.buf = NULL;
  return s;
}

sag_string sag_string_empty(void) {
  return sag_string_from_static("", 0);
}

void sag_string_retain(sag_string s) {
  if (s.buf != NULL) {
    sag_count_inc(&s.buf->refcount);
  }
}

void sag_string_release(sag_string s) {
  if (s.buf != NULL && sag_count_dec(&s.buf->refcount) == 0) {
    free(s.buf);
    sag_count_dec(&sag_live_objects);
  }
}

static void *sag_alloc(size_t size) {
  void *memory = malloc(size);
  if (memory == NULL) {
    sag_panic("out of memory");
  }
  return memory;
}

void sag_sb_init(sag_sb *sb) {
  sb->data = NULL;
  sb->len = 0;
  sb->cap = 0;
}

void sag_sb_push_bytes(sag_sb *sb, const char *bytes, size_t len) {
  if (len == 0) {
    return;
  }
  if (len > SIZE_MAX - sb->len) {
    sag_panic("string too large");
  }
  if (sb->len + len > sb->cap) {
    size_t cap = sb->cap == 0 ? 32 : sb->cap;
    while (cap < sb->len + len) {
      cap = cap > SIZE_MAX / 2 ? sb->len + len : cap * 2;
    }
    uint8_t *data = realloc(sb->data, cap);
    if (data == NULL) {
      sag_panic("out of memory");
    }
    sb->data = data;
    sb->cap = cap;
  }
  memcpy(sb->data + sb->len, bytes, len);
  sb->len += len;
}

void sag_sb_push_str(sag_sb *sb, sag_string s) {
  sag_sb_push_bytes(sb, (const char *)s.ptr, s.len);
}

void sag_sb_push_i64(sag_sb *sb, int64_t value) {
  char text[32];
  int len = snprintf(text, sizeof text, "%" PRId64, value);
  sag_sb_push_bytes(sb, text, (size_t)len);
}

void sag_sb_push_u64(sag_sb *sb, uint64_t value) {
  char text[32];
  int len = snprintf(text, sizeof text, "%" PRIu64, value);
  sag_sb_push_bytes(sb, text, (size_t)len);
}

static void sag_push_zeros(sag_sb *sb, int count) {
  for (int i = 0; i < count; i++) {
    sag_sb_push_bytes(sb, "0", 1);
  }
}

/* Finds the fewest significant digits that read back as the same value, then writes them in
 * positional notation (Rust's `Display` for floats). */
static void sag_push_float(sag_sb *sb, double value, int max_digits, bool is_f32) {
  if (value != value) {
    sag_sb_push_bytes(sb, "NaN", 3);
    return;
  }
  if (isinf(value)) {
    if (value < 0) {
      sag_sb_push_bytes(sb, "-inf", 4);
    } else {
      sag_sb_push_bytes(sb, "inf", 3);
    }
    return;
  }
  if (value == 0) {
    if (signbit(value)) {
      sag_sb_push_bytes(sb, "-0", 2);
    } else {
      sag_sb_push_bytes(sb, "0", 1);
    }
    return;
  }

  char text[64];
  for (int digits = 1; digits <= max_digits; digits++) {
    snprintf(text, sizeof text, "%.*e", digits - 1, value);
    double back = strtod(text, NULL);
    if (is_f32 ? (float)back == (float)value : back == value) {
      break;
    }
  }

  /* `text` looks like "-1.2345e+02": collect the digits and the exponent. */
  const char *cursor = text;
  bool negative = false;
  if (*cursor == '-') {
    negative = true;
    cursor++;
  }
  char digits[32];
  int count = 0;
  while (*cursor != '\0' && *cursor != 'e' && count < (int)sizeof digits) {
    if (*cursor >= '0' && *cursor <= '9') {
      digits[count++] = *cursor;
    }
    cursor++;
  }
  int exponent = *cursor == 'e' ? atoi(cursor + 1) : 0;
  while (count > 1 && digits[count - 1] == '0') {
    count--;
  }

  /* The decimal point goes after `point` digits. */
  int point = exponent + 1;
  if (negative) {
    sag_sb_push_bytes(sb, "-", 1);
  }
  if (point <= 0) {
    sag_sb_push_bytes(sb, "0.", 2);
    sag_push_zeros(sb, -point);
    sag_sb_push_bytes(sb, digits, (size_t)count);
  } else if (point >= count) {
    sag_sb_push_bytes(sb, digits, (size_t)count);
    sag_push_zeros(sb, point - count);
  } else {
    sag_sb_push_bytes(sb, digits, (size_t)point);
    sag_sb_push_bytes(sb, ".", 1);
    sag_sb_push_bytes(sb, digits + point, (size_t)(count - point));
  }
}

/* The last value given to `black_box`, written through a `volatile` so the write stays. */
static const void *volatile sag_black_box_sink = NULL;

void sag_black_box(const void *value) {
  sag_black_box_sink = value;
}

sag_string sag_float_format(double value, int32_t precision, bool scientific, bool upper, bool is_f32) {
  sag_sb sb;
  sag_sb_init(&sb);
  if (value != value) {
    sag_sb_push_bytes(&sb, "NaN", 3);
    return sag_sb_finish(&sb);
  }
  double magnitude = fabs(value);
  if (isinf(magnitude)) {
    sag_sb_push_bytes(&sb, "inf", 3);
    return sag_sb_finish(&sb);
  }
  if (!scientific) {
    if (precision < 0) {
      sag_push_float(&sb, magnitude, is_f32 ? 9 : 17, is_f32);
    } else {
      /* C rounds the exact value to the nearest, as Rust does. */
      int count = snprintf(NULL, 0, "%.*f", (int)precision, magnitude);
      char *text = sag_alloc((size_t)count + 1);
      snprintf(text, (size_t)count + 1, "%.*f", (int)precision, magnitude);
      sag_sb_push_bytes(&sb, text, (size_t)count);
      free(text);
    }
    return sag_sb_finish(&sb);
  }
  /* Scientific: the fewest digits that read back as the value, or `precision` decimals. */
  char text[512];
  if (precision < 0) {
    int max_digits = is_f32 ? 9 : 17;
    for (int digits = 1; digits <= max_digits; digits++) {
      snprintf(text, sizeof text, "%.*e", digits - 1, magnitude);
      double back = strtod(text, NULL);
      if (is_f32 ? (float)back == (float)magnitude : back == magnitude) {
        break;
      }
    }
  } else {
    snprintf(text, sizeof text, "%.*e", precision > 400 ? 400 : (int)precision, magnitude);
  }
  /* `text` looks like "1.2340e+02": the mantissa, without trailing zeros unless a precision
   * asks for them, then `e` and the exponent without `+` or leading zeros. */
  char *mark = strchr(text, 'e');
  size_t mantissa = mark != NULL ? (size_t)(mark - text) : strlen(text);
  if (precision < 0 && memchr(text, '.', mantissa) != NULL) {
    while (mantissa > 0 && text[mantissa - 1] == '0') {
      mantissa--;
    }
    if (mantissa > 0 && text[mantissa - 1] == '.') {
      mantissa--;
    }
  }
  sag_sb_push_bytes(&sb, text, mantissa);
  sag_sb_push_bytes(&sb, upper ? "E" : "e", 1);
  long exponent = mark != NULL ? strtol(mark + 1, NULL, 10) : 0;
  char digits[32];
  int written = snprintf(digits, sizeof digits, "%ld", exponent);
  sag_sb_push_bytes(&sb, digits, (size_t)written);
  return sag_sb_finish(&sb);
}

void sag_sb_push_f64(sag_sb *sb, double value) {
  sag_push_float(sb, value, 17, false);
}

void sag_sb_push_f32(sag_sb *sb, float value) {
  sag_push_float(sb, (double)value, 9, true);
}

void sag_sb_push_bool(sag_sb *sb, bool value) {
  if (value) {
    sag_sb_push_bytes(sb, "true", 4);
  } else {
    sag_sb_push_bytes(sb, "false", 5);
  }
}

/* ---- The debug form of values (docs/adr/0089-display-debug-and-format-specifiers.md) -------- */

/* Rust's escapes of `{:?}`: quotes, backslashes, `\n`, `\r`, `\t`, `\0`, and `\u{...}` for the
 * other control characters. */
static void sag_push_debug_scalar(sag_sb *sb, uint32_t c, uint32_t quote) {
  char escaped[16];
  if (c == quote || c == '\\') {
    escaped[0] = '\\';
    escaped[1] = (char)c;
    sag_sb_push_bytes(sb, escaped, 2);
  } else if (c == '\n') {
    sag_sb_push_bytes(sb, "\\n", 2);
  } else if (c == '\r') {
    sag_sb_push_bytes(sb, "\\r", 2);
  } else if (c == '\t') {
    sag_sb_push_bytes(sb, "\\t", 2);
  } else if (c == 0) {
    sag_sb_push_bytes(sb, "\\0", 2);
  } else if (c < 0x20 || c == 0x7F) {
    int len = snprintf(escaped, sizeof escaped, "\\u{%x}", (unsigned)c);
    sag_sb_push_bytes(sb, escaped, (size_t)len);
  } else {
    sag_sb_push_char(sb, c);
  }
}

void sag_sb_push_debug_str(sag_sb *sb, sag_string s) {
  sag_sb_push_bytes(sb, "\"", 1);
  size_t index = 0;
  while (index < s.len) {
    sag_push_debug_scalar(sb, sag_utf8_next(s, &index), '"');
  }
  sag_sb_push_bytes(sb, "\"", 1);
}

void sag_sb_push_debug_char(sag_sb *sb, uint32_t c) {
  sag_sb_push_bytes(sb, "'", 1);
  sag_push_debug_scalar(sb, c, '\'');
  sag_sb_push_bytes(sb, "'", 1);
}

void sag_sb_push_address(sag_sb *sb, const void *address) {
  char text[32];
  int len = snprintf(text, sizeof text, "0x%" PRIxPTR, (uintptr_t)address);
  sag_sb_push_bytes(sb, text, (size_t)len);
}

static void sag_debug_line(sag_sb *sb, int32_t indent) {
  sag_sb_push_bytes(sb, "\n", 1);
  for (int32_t level = 0; level < indent; level++) {
    sag_sb_push_bytes(sb, "    ", 4);
  }
}

void sag_debug_next(sag_sb *sb, bool first, int32_t indent, bool pretty, bool spaced) {
  if (pretty) {
    sag_debug_line(sb, indent);
  } else if (!first) {
    sag_sb_push_bytes(sb, ", ", 2);
  } else if (spaced) {
    sag_sb_push_bytes(sb, " ", 1);
  }
}

void sag_debug_end(sag_sb *sb, bool any, int32_t indent, bool pretty, bool spaced, const char *close) {
  if (pretty && any) {
    sag_debug_line(sb, indent);
  } else if (spaced && any) {
    sag_sb_push_bytes(sb, " ", 1);
  }
  sag_sb_push_bytes(sb, close, strlen(close));
}

/* The objects whose debug form this thread is writing, innermost last. */
static SAG_THREAD_LOCAL const void **sag_debug_objects = NULL;
static SAG_THREAD_LOCAL size_t sag_debug_len = 0;
static SAG_THREAD_LOCAL size_t sag_debug_cap = 0;

bool sag_debug_enter(const void *object) {
  for (size_t index = 0; index < sag_debug_len; index++) {
    if (sag_debug_objects[index] == object) {
      return false;
    }
  }
  if (sag_debug_len == sag_debug_cap) {
    size_t cap = sag_debug_cap == 0 ? 8 : sag_debug_cap * 2;
    const void **grown = realloc((void *)sag_debug_objects, cap * sizeof(const void *));
    if (grown == NULL) {
      sag_panic("out of memory");
    }
    sag_debug_objects = grown;
    sag_debug_cap = cap;
  }
  sag_debug_objects[sag_debug_len++] = object;
  return true;
}

void sag_debug_leave(void) {
  if (sag_debug_len > 0) {
    sag_debug_len--;
  }
  /* Freed when the outermost form ends, so leak checkers see nothing left. */
  if (sag_debug_len == 0) {
    free((void *)sag_debug_objects);
    sag_debug_objects = NULL;
    sag_debug_cap = 0;
  }
}

void sag_sb_push_char(sag_sb *sb, uint32_t c) {
  char bytes[4];
  size_t len;
  if (c < 0x80) {
    bytes[0] = (char)c;
    len = 1;
  } else if (c < 0x800) {
    bytes[0] = (char)(0xC0 | (c >> 6));
    bytes[1] = (char)(0x80 | (c & 0x3F));
    len = 2;
  } else if (c < 0x10000) {
    bytes[0] = (char)(0xE0 | (c >> 12));
    bytes[1] = (char)(0x80 | ((c >> 6) & 0x3F));
    bytes[2] = (char)(0x80 | (c & 0x3F));
    len = 3;
  } else {
    bytes[0] = (char)(0xF0 | (c >> 18));
    bytes[1] = (char)(0x80 | ((c >> 12) & 0x3F));
    bytes[2] = (char)(0x80 | ((c >> 6) & 0x3F));
    bytes[3] = (char)(0x80 | (c & 0x3F));
    len = 4;
  }
  sag_sb_push_bytes(sb, bytes, len);
}

sag_string sag_sb_finish(sag_sb *sb) {
  if (sb->len == 0) {
    free(sb->data);
    sag_sb_init(sb);
    return sag_string_empty();
  }
  sag_string_buf *buf = sag_alloc(sizeof(sag_string_buf) + sb->len);
  sag_count_init(&buf->refcount, 1);
  buf->len = sb->len;
  memcpy(buf->bytes, sb->data, sb->len);
  free(sb->data);
  sag_sb_init(sb);
  sag_count_inc(&sag_live_objects);

  sag_string s;
  s.ptr = buf->bytes;
  s.len = buf->len;
  s.buf = buf;
  return s;
}

/* ---- Printing (prelude) ---------------------------------------------------------------- */

static void sag_write_or_panic(FILE *stream, const uint8_t *bytes, size_t len) {
  if (len > 0 && fwrite(bytes, 1, len, stream) != len) {
    sag_panic(stream == stderr ? "failed printing to stderr" : "failed printing to stdout");
  }
}

void sag_print(sag_string s) {
  sag_write_or_panic(stdout, s.ptr, s.len);
}

void sag_println(sag_string s) {
  sag_write_or_panic(stdout, s.ptr, s.len);
  sag_write_or_panic(stdout, (const uint8_t *)"\n", 1);
}

void sag_eprintln(sag_string s) {
  sag_write_or_panic(stderr, s.ptr, s.len);
  sag_write_or_panic(stderr, (const uint8_t *)"\n", 1);
}

void sag_report_main_error(sag_string message) {
  fflush(stdout);
  fputs("error: ", stderr);
  sag_write_or_panic(stderr, message.ptr, message.len);
  sag_write_or_panic(stderr, (const uint8_t *)"\n", 1);
}

/* ---- String API -------------------------------------------------------------------------- */

/* A new string holding a copy of `len` bytes. */
static sag_string sag_string_copy(const uint8_t *bytes, size_t len) {
  if (len == 0) {
    return sag_string_empty();
  }
  sag_sb sb;
  sag_sb_init(&sb);
  sag_sb_push_bytes(&sb, (const char *)bytes, len);
  return sag_sb_finish(&sb);
}

/* A view of `len` bytes of `s` starting at `start`, sharing (and retaining) its buffer. */
static sag_string sag_string_view(sag_string s, size_t start, size_t len) {
  sag_string view;
  view.ptr = s.ptr + start;
  view.len = len;
  view.buf = s.buf;
  sag_string_retain(view);
  return view;
}

sag_string sag_string_concat(sag_string a, sag_string b) {
  sag_sb sb;
  sag_sb_init(&sb);
  sag_sb_push_str(&sb, a);
  sag_sb_push_str(&sb, b);
  return sag_sb_finish(&sb);
}

int sag_string_cmp(sag_string a, sag_string b) {
  size_t shorter = a.len < b.len ? a.len : b.len;
  int result = shorter == 0 ? 0 : memcmp(a.ptr, b.ptr, shorter);
  if (result != 0) {
    return result;
  }
  return a.len < b.len ? -1 : (a.len > b.len ? 1 : 0);
}

size_t sag_string_find(sag_string s, sag_string pattern) {
  if (pattern.len == 0) {
    return 0;
  }
  for (size_t i = 0; pattern.len <= s.len && i <= s.len - pattern.len; i++) {
    if (memcmp(s.ptr + i, pattern.ptr, pattern.len) == 0) {
      return i;
    }
  }
  return SIZE_MAX;
}

bool sag_string_contains(sag_string s, sag_string pattern) {
  return sag_string_find(s, pattern) != SIZE_MAX;
}

bool sag_string_starts_with(sag_string s, sag_string prefix) {
  return prefix.len <= s.len && (prefix.len == 0 || memcmp(s.ptr, prefix.ptr, prefix.len) == 0);
}

bool sag_string_ends_with(sag_string s, sag_string suffix) {
  return suffix.len <= s.len && (suffix.len == 0 || memcmp(s.ptr + s.len - suffix.len, suffix.ptr, suffix.len) == 0);
}

/* Unicode White_Space characters, as Rust's `trim` removes. */
static bool sag_is_unicode_whitespace(uint32_t c) {
  return (c >= 0x09 && c <= 0x0D) || c == 0x20 || c == 0x85 || c == 0xA0 || c == 0x1680 || (c >= 0x2000 && c <= 0x200A) || c == 0x2028 ||
         c == 0x2029 || c == 0x202F || c == 0x205F || c == 0x3000;
}

static size_t sag_trimmed_start(sag_string s) {
  size_t index = 0;
  while (index < s.len) {
    size_t next = index;
    if (!sag_is_unicode_whitespace(sag_utf8_next(s, &next))) {
      break;
    }
    index = next;
  }
  return index;
}

static size_t sag_trimmed_end(sag_string s, size_t start) {
  size_t end = start;
  size_t index = start;
  while (index < s.len) {
    if (!sag_is_unicode_whitespace(sag_utf8_next(s, &index))) {
      end = index;
    }
  }
  return end;
}

sag_string sag_string_trim(sag_string s) {
  size_t start = sag_trimmed_start(s);
  return sag_string_view(s, start, sag_trimmed_end(s, start) - start);
}

sag_string sag_string_trim_start(sag_string s) {
  size_t start = sag_trimmed_start(s);
  return sag_string_view(s, start, s.len - start);
}

sag_string sag_string_trim_end(sag_string s) {
  return sag_string_view(s, 0, sag_trimmed_end(s, 0));
}

static sag_string sag_map_ascii(sag_string s, uint32_t (*map)(uint32_t)) {
  sag_sb sb;
  sag_sb_init(&sb);
  for (size_t i = 0; i < s.len; i++) {
    char byte = (char)map(s.ptr[i]);
    sag_sb_push_bytes(&sb, &byte, 1);
  }
  return sag_sb_finish(&sb);
}

static uint32_t sag_upper_byte(uint32_t c) {
  return sag_char_to_ascii_uppercase(c);
}

static uint32_t sag_lower_byte(uint32_t c) {
  return sag_char_to_ascii_lowercase(c);
}

sag_string sag_string_to_ascii_uppercase(sag_string s) {
  return sag_map_ascii(s, sag_upper_byte);
}

sag_string sag_string_to_ascii_lowercase(sag_string s) {
  return sag_map_ascii(s, sag_lower_byte);
}

sag_string sag_string_replace(sag_string s, sag_string from, sag_string to) {
  sag_sb sb;
  sag_sb_init(&sb);
  if (from.len == 0) {
    /* Like Rust: `to` goes before every character and at the end. */
    size_t index = 0;
    while (index < s.len) {
      size_t start = index;
      sag_utf8_next(s, &index);
      sag_sb_push_str(&sb, to);
      sag_sb_push_bytes(&sb, (const char *)s.ptr + start, index - start);
    }
    sag_sb_push_str(&sb, to);
    return sag_sb_finish(&sb);
  }
  size_t index = 0;
  while (index < s.len) {
    if (from.len <= s.len - index && memcmp(s.ptr + index, from.ptr, from.len) == 0) {
      sag_sb_push_str(&sb, to);
      index += from.len;
    } else {
      sag_sb_push_bytes(&sb, (const char *)s.ptr + index, 1);
      index++;
    }
  }
  return sag_sb_finish(&sb);
}

sag_string sag_string_repeat_at(sag_string s, size_t count, const char *file, int line) {
  if (s.len != 0 && count > SIZE_MAX / s.len) {
    sag_panic_at("capacity overflow", file, line);
  }
  sag_sb sb;
  sag_sb_init(&sb);
  for (size_t i = 0; i < count; i++) {
    sag_sb_push_str(&sb, s);
  }
  return sag_sb_finish(&sb);
}

static bool sag_is_char_boundary(sag_string s, size_t index) {
  return index == 0 || index == s.len || (index < s.len && (s.ptr[index] & 0xC0) != 0x80);
}

sag_string sag_string_substring_at(sag_string s, size_t start, size_t end, const char *file, int line) {
  char message[128];
  if (start > end) {
    snprintf(message, sizeof message, "begin <= end (%zu <= %zu) when slicing a string", start, end);
    sag_panic_at(message, file, line);
  }
  if (end > s.len) {
    snprintf(message, sizeof message, "byte index %zu is out of bounds of a string of length %zu", end, s.len);
    sag_panic_at(message, file, line);
  }
  size_t bad = !sag_is_char_boundary(s, start) ? start : end;
  if (!sag_is_char_boundary(s, start) || !sag_is_char_boundary(s, end)) {
    snprintf(message, sizeof message, "byte index %zu is not a char boundary", bad);
    sag_panic_at(message, file, line);
  }
  return sag_string_view(s, start, end - start);
}

uint32_t sag_utf8_next(sag_string s, size_t *index) {
  const uint8_t *p = s.ptr + *index;
  uint32_t c = p[0];
  size_t width = 1;
  if (c >= 0xF0) {
    c = ((c & 0x07) << 18) | ((uint32_t)(p[1] & 0x3F) << 12) | ((uint32_t)(p[2] & 0x3F) << 6) | (uint32_t)(p[3] & 0x3F);
    width = 4;
  } else if (c >= 0xE0) {
    c = ((c & 0x0F) << 12) | ((uint32_t)(p[1] & 0x3F) << 6) | (uint32_t)(p[2] & 0x3F);
    width = 3;
  } else if (c >= 0xC0) {
    c = ((c & 0x1F) << 6) | (uint32_t)(p[1] & 0x3F);
    width = 2;
  }
  *index += width;
  return c;
}

/* ---- Parsing ------------------------------------------------------------------------------ */

static bool sag_parse_error(sag_string *error, const char *message) {
  *error = sag_string_copy((const uint8_t *)message, strlen(message));
  return false;
}

/* Parses an optional sign and decimal digits into a magnitude, like Rust's `from_str`. */
static bool sag_parse_digits(sag_string s, bool allow_minus, bool *negative, uint64_t limit, uint64_t *magnitude, sag_string *error) {
  size_t i = 0;
  *negative = false;
  *magnitude = 0;
  if (s.len == 0) {
    return sag_parse_error(error, "cannot parse integer from empty string");
  }
  if (s.ptr[0] == '+' || (allow_minus && s.ptr[0] == '-')) {
    *negative = s.ptr[0] == '-';
    i = 1;
    if (s.len == 1) {
      return sag_parse_error(error, "invalid digit found in string");
    }
  }
  for (; i < s.len; i++) {
    uint8_t c = s.ptr[i];
    if (c < '0' || c > '9') {
      return sag_parse_error(error, "invalid digit found in string");
    }
    uint64_t digit = (uint64_t)(c - '0');
    if (*magnitude > (limit - digit) / 10) {
      return sag_parse_error(error, *negative ? "number too small to fit in target type" : "number too large to fit in target type");
    }
    *magnitude = *magnitude * 10 + digit;
  }
  return true;
}

bool sag_parse_signed(sag_string s, int64_t min, int64_t max, int64_t *value, sag_string *error) {
  bool negative;
  uint64_t magnitude;
  /* The magnitude of `min` is `max + 1`. */
  if (!sag_parse_digits(s, true, &negative, (uint64_t)max + 1, &magnitude, error)) {
    return false;
  }
  if (!negative && magnitude > (uint64_t)max) {
    return sag_parse_error(error, "number too large to fit in target type");
  }
  (void)min;
  *value = negative ? (magnitude == 0 ? 0 : -(int64_t)(magnitude - 1) - 1) : (int64_t)magnitude;
  *error = sag_string_empty();
  return true;
}

bool sag_parse_unsigned(sag_string s, uint64_t max, uint64_t *value, sag_string *error) {
  bool negative;
  if (!sag_parse_digits(s, false, &negative, max, value, error)) {
    return false;
  }
  *error = sag_string_empty();
  return true;
}

bool sag_parse_float(sag_string s, double *value, sag_string *error) {
  char text[128];
  if (s.len == 0 || s.len >= sizeof text) {
    return sag_parse_error(error, s.len == 0 ? "cannot parse float from empty string" : "invalid float literal");
  }
  for (size_t i = 0; i < s.len; i++) {
    uint8_t c = s.ptr[i];
    /* strtod also accepts leading spaces and hexadecimal floats, which Rust rejects. */
    if (c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\f' || c == '\v' || c == 'x' || c == 'X' || c == 'p' || c == 'P') {
      return sag_parse_error(error, "invalid float literal");
    }
    text[i] = (char)c;
  }
  text[s.len] = '\0';
  char *end = NULL;
  double parsed = strtod(text, &end);
  if (end != text + s.len) {
    return sag_parse_error(error, "invalid float literal");
  }
  *value = parsed;
  *error = sag_string_empty();
  return true;
}

uint32_t sag_char_to_digit_at(uint32_t c, uint32_t radix, const char *file, int line) {
  if (radix < 2 || radix > 36) {
    sag_panic_at("to_digit: radix is too high (maximum 36)", file, line);
  }
  uint32_t digit = UINT32_MAX;
  if (c >= '0' && c <= '9') {
    digit = c - '0';
  } else if (c >= 'a' && c <= 'z') {
    digit = c - 'a' + 10;
  } else if (c >= 'A' && c <= 'Z') {
    digit = c - 'A' + 10;
  }
  return digit < radix ? digit : UINT32_MAX;
}

/* ---- StringBuilder ----------------------------------------------------------------------- */

struct sag_builder_obj {
  sag_count refcount;
  sag_sb sb;
};

sag_builder sag_builder_new(void) {
  sag_builder builder = sag_alloc(sizeof(struct sag_builder_obj));
  sag_count_init(&builder->refcount, 1);
  sag_sb_init(&builder->sb);
  sag_count_inc(&sag_live_objects);
  return builder;
}

void sag_builder_retain(sag_builder builder) {
  if (builder != NULL) {
    sag_count_inc(&builder->refcount);
  }
}

void sag_builder_release(sag_builder builder) {
  if (builder != NULL && sag_count_dec(&builder->refcount) == 0) {
    free(builder->sb.data);
    free(builder);
    sag_count_dec(&sag_live_objects);
  }
}

void sag_builder_append(sag_builder builder, sag_string s) {
  sag_sb_push_str(&builder->sb, s);
}

void sag_builder_append_char(sag_builder builder, uint32_t c) {
  sag_sb_push_char(&builder->sb, c);
}

size_t sag_builder_len(sag_builder builder) {
  return builder->sb.len;
}

bool sag_builder_is_empty(sag_builder builder) {
  return builder->sb.len == 0;
}

void sag_builder_clear(sag_builder builder) {
  builder->sb.len = 0;
}

sag_string sag_builder_to_string(sag_builder builder) {
  return sag_string_copy(builder->sb.data, builder->sb.len);
}

/* ---- Class instances --------------------------------------------------------------------- */

/* Objects whose count reached zero and wait to be destroyed, and whether destruction is under
 * way. Single-threaded MVP. */
/* Each thread destroys the objects it releases, with its own stack. */
static SAG_THREAD_LOCAL sag_object **sag_drop_stack = NULL;
static SAG_THREAD_LOCAL size_t sag_drop_len = 0;
static SAG_THREAD_LOCAL size_t sag_drop_cap = 0;
static SAG_THREAD_LOCAL bool sag_dropping = false;

/* Frees the stack of this thread, when it ends. */
static void sag_drop_free(void) {
  free(sag_drop_stack);
  sag_drop_stack = NULL;
  sag_drop_cap = 0;
}

void *sag_object_alloc(size_t size) {
  return sag_alloc(size);
}

void sag_object_init(sag_object *object, void (*drop)(sag_object *object), const sag_method *vtable) {
  sag_count_init(&object->strong, 1);
  sag_count_init(&object->weak, 1);
  object->drop = drop;
  object->vtable = vtable;
  sag_count_inc(&sag_live_objects);
}


static void sag_drop_push(sag_object *object) {
  if (sag_drop_len == sag_drop_cap) {
    size_t cap = sag_drop_cap == 0 ? 16 : sag_drop_cap;
    if (cap > SIZE_MAX / 2 / sizeof(sag_object *)) {
      sag_panic("out of memory");
    }
    cap *= 2;
    sag_object **stack = realloc(sag_drop_stack, cap * sizeof(sag_object *));
    if (stack == NULL) {
      sag_panic("out of memory");
    }
    sag_drop_stack = stack;
    sag_drop_cap = cap;
  }
  sag_drop_stack[sag_drop_len++] = object;
}

void sag_object_destroy(sag_object *object) {
  sag_drop_push(object);
  if (sag_dropping) {
    return;
  }
  sag_dropping = true;
  while (sag_drop_len > 0) {
    sag_object *next = sag_drop_stack[--sag_drop_len];
    next->drop(next);
    /* The weak reference of the strong ones, kept while the fields were released. */
    sag_object_weak_release(next);
  }
  sag_dropping = false;
}

static void sag_object_weak_retain(sag_object *object) {
  if (object != NULL) {
    sag_count_inc(&object->weak);
  }
}

void sag_object_weak_release(sag_object *object) {
  if (object != NULL && sag_count_dec(&object->weak) == 0) {
    free(object);
    sag_count_dec(&sag_live_objects);
  }
}

void sag_object_weak_set(sag_object **slot, sag_object *object) {
  sag_object_weak_retain(object);
  sag_object_weak_release(*slot);
  *slot = object;
}

sag_object *sag_object_upgrade(sag_object *object) {
  return object != NULL && sag_count_inc_nonzero(&object->strong) ? object : NULL;
}

/* ---- Lists ------------------------------------------------------------------------------- */

static void sag_list_drop(sag_object *object) {
  sag_list *list = (sag_list *)object;
  if (list->release != NULL) {
    for (size_t i = 0; i < list->len; i++) {
      list->release(list->data + i * list->size);
    }
  }
  free(list->data);
}

/* Makes room for `cap` elements plus the spare slot. */
static void sag_list_reserve(sag_list *list, size_t cap) {
  if (cap <= list->cap) {
    return;
  }
  if (cap > SIZE_MAX / list->size - 1) {
    sag_panic("out of memory");
  }
  unsigned char *data = realloc(list->data, (cap + 1) * list->size);
  if (data == NULL) {
    sag_panic("out of memory");
  }
  list->data = data;
  list->cap = cap;
}

/* Room for one more element. */
static void sag_list_grow(sag_list *list) {
  if (list->len < list->cap) {
    return;
  }
  sag_list_reserve(list, list->cap == 0 ? 4 : (list->cap > SIZE_MAX / 2 ? SIZE_MAX : list->cap * 2));
}

sag_object *sag_list_new(size_t size, void (*release)(void *element)) {
  sag_list *list = sag_alloc(sizeof(sag_list));
  list->len = 0;
  list->cap = 0;
  /* `()` elements still take a byte, as in every other C value. */
  list->size = size == 0 ? 1 : size;
  list->data = NULL;
  list->release = release;
  list->lent = 0;
  sag_object_init(&list->header, sag_list_drop, NULL);
  return &list->header;
}

sag_object *sag_list_from(size_t size, void (*release)(void *element), const void *elements, size_t len) {
  sag_object *object = sag_list_new(size, release);
  sag_list *list = (sag_list *)object;
  if (len > 0) {
    sag_list_reserve(list, len);
    memcpy(list->data, elements, len * list->size);
    list->len = len;
  }
  return object;
}

/* A list lent as a slice keeps its length, so that its elements do not move. */
static void sag_list_check_unlent(sag_list *list) {
  if (list->lent > 0) {
    sag_panic("cannot change the length of a list while it is borrowed");
  }
}

void *sag_list_push(sag_object *object) {
  sag_list *list = (sag_list *)object;
  sag_list_check_unlent(list);
  sag_list_grow(list);
  return list->data + list->len++ * list->size;
}

void *sag_list_insert(sag_object *object, size_t index, const char *file, int line) {
  sag_list *list = (sag_list *)object;
  sag_list_check_unlent(list);
  if (index > list->len) {
    char message[128];
    snprintf(message, sizeof message, "insertion index (is %zu) should be <= len (is %zu)", index, list->len);
    sag_panic_at(message, file, line);
  }
  sag_list_grow(list);
  unsigned char *slot = list->data + index * list->size;
  memmove(slot + list->size, slot, (list->len - index) * list->size);
  list->len++;
  return slot;
}

void *sag_list_pop(sag_object *object) {
  sag_list *list = (sag_list *)object;
  sag_list_check_unlent(list);
  if (list->len == 0) {
    sag_panic("pop from an empty list");
  }
  return list->data + --list->len * list->size;
}

void *sag_list_remove(sag_object *object, size_t index, const char *file, int line) {
  sag_list *list = (sag_list *)object;
  sag_list_check_unlent(list);
  if (index >= list->len) {
    char message[128];
    snprintf(message, sizeof message, "removal index (is %zu) should be < len (is %zu)", index, list->len);
    sag_panic_at(message, file, line);
  }
  unsigned char *spare = list->data + list->cap * list->size;
  unsigned char *slot = list->data + index * list->size;
  memcpy(spare, slot, list->size);
  memmove(slot, slot + list->size, (list->len - index - 1) * list->size);
  list->len--;
  return spare;
}

void sag_list_clear(sag_object *object) {
  sag_list *list = (sag_list *)object;
  sag_list_check_unlent(list);
  size_t len = list->len;
  /* Emptied first: releasing an element cannot reach the list again. */
  list->len = 0;
  if (list->release != NULL) {
    for (size_t i = 0; i < len; i++) {
      list->release(list->data + i * list->size);
    }
  }
}

/* ---- Slices ---------------------------------------------------------------------------- */

/* Checks `start..end` (or `start..=end`) against `len` with Rust's messages, and returns the
 * number of elements; `*first` gets the first one. */
static size_t sag_slice_range(size_t len, bool whole, size_t start, size_t end, bool inclusive, size_t *first, const char *file, int line) {
  if (whole) {
    *first = 0;
    return len;
  }
  char message[128];
  if (inclusive) {
    if (end == SIZE_MAX) {
      sag_panic_at("attempted to index slice up to maximum usize", file, line);
    }
    end++;
  }
  if (start > end) {
    snprintf(message, sizeof message, "slice index starts at %zu but ends at %zu", start, end);
    sag_panic_at(message, file, line);
  }
  if (end > len) {
    snprintf(message, sizeof message, "range end index %zu out of range for slice of length %zu", end, len);
    sag_panic_at(message, file, line);
  }
  *first = start;
  return end - start;
}

/* `count` elements from the element `first` of `data`; a null `data` holds no elements. */
static sag_slice sag_slice_make(void *data, size_t size, size_t first, size_t count, sag_object *owner) {
  sag_slice slice;
  slice.data = data == NULL ? NULL : (unsigned char *)data + first * size;
  slice.len = count;
  slice.owner = owner;
  return slice;
}

sag_slice sag_slice_list(sag_object *object, bool whole, size_t start, size_t end, bool inclusive, const char *file, int line) {
  sag_list *list = (sag_list *)object;
  size_t first = 0;
  size_t count = sag_slice_range(list->len, whole, start, end, inclusive, &first, file, line);
  sag_object_retain(object);
  list->lent++;
  return sag_slice_make(list->data, list->size, first, count, object);
}

sag_slice sag_slice_array(void *data, size_t len, size_t size, bool whole, size_t start, size_t end, bool inclusive, const char *file, int line) {
  size_t first = 0;
  size_t count = sag_slice_range(len, whole, start, end, inclusive, &first, file, line);
  return sag_slice_make(data, size, first, count, NULL);
}

sag_slice sag_slice_sub(sag_slice slice, size_t size, bool whole, size_t start, size_t end, bool inclusive, const char *file, int line) {
  size_t first = 0;
  size_t count = sag_slice_range(slice.len, whole, start, end, inclusive, &first, file, line);
  sag_slice part = sag_slice_make(slice.data, size, first, count, slice.owner);
  sag_slice_retain(part);
  return part;
}

void sag_slice_retain(sag_slice slice) {
  if (slice.owner != NULL) {
    sag_object_retain(slice.owner);
    ((sag_list *)slice.owner)->lent++;
  }
}

void sag_slice_release(sag_slice slice) {
  if (slice.owner != NULL) {
    ((sag_list *)slice.owner)->lent--;
    sag_object_release(slice.owner);
  }
}

void sag_list_reverse(sag_object *object) {
  sag_list *list = (sag_list *)object;
  if (list->len < 2) {
    return;
  }
  unsigned char *spare = list->data + list->cap * list->size;
  for (size_t i = 0, j = list->len - 1; i < j; i++, j--) {
    memcpy(spare, list->data + i * list->size, list->size);
    memcpy(list->data + i * list->size, list->data + j * list->size, list->size);
    memcpy(list->data + j * list->size, spare, list->size);
  }
}

/* ---- Hashing and order ------------------------------------------------------------------- */

/* The finalizer of SplitMix64: every input bit affects every output bit. */
/* FNV-1a. */
uint64_t sag_string_hash(sag_string text) {
  uint64_t hash = UINT64_C(0xcbf29ce484222325);
  for (size_t i = 0; i < text.len; i++) {
    hash = (hash ^ text.ptr[i]) * UINT64_C(0x100000001b3);
  }
  return hash;
}

/* Maps the bits of a float to an unsigned key with the same total order. */
static uint64_t sag_total_key64(uint64_t bits) {
  return (bits >> 63) != 0 ? ~bits : bits | (UINT64_C(1) << 63);
}

static uint32_t sag_total_key32(uint32_t bits) {
  return (bits >> 31) != 0 ? ~bits : bits | (UINT32_C(1) << 31);
}

int32_t sag_f32_total_compare(float a, float b) {
  uint32_t x;
  uint32_t y;
  memcpy(&x, &a, sizeof x);
  memcpy(&y, &b, sizeof y);
  x = sag_total_key32(x);
  y = sag_total_key32(y);
  return (x > y) - (x < y);
}

int32_t sag_f64_total_compare(double a, double b) {
  uint64_t x;
  uint64_t y;
  memcpy(&x, &a, sizeof x);
  memcpy(&y, &b, sizeof y);
  x = sag_total_key64(x);
  y = sag_total_key64(y);
  return (x > y) - (x < y);
}

/* ---- FFI --------------------------------------------------------------------------------- */

struct sag_cstring_obj {
  sag_count refcount;
  char bytes[];
};

_Noreturn void sag_null_pointer_at(const char *file, int line) {
  sag_panic_at("null pointer dereference", file, line);
}

bool sag_cstring_new(sag_string text, sag_cstring *out, sag_string *error) {
  const void *nul = text.len == 0 ? NULL : memchr(text.ptr, 0, text.len);
  if (nul != NULL) {
    static const char prefix[] = "nul byte found in provided data at position: ";
    sag_sb sb;
    sag_sb_init(&sb);
    sag_sb_push_bytes(&sb, prefix, sizeof prefix - 1);
    sag_sb_push_u64(&sb, (uint64_t)((const uint8_t *)nul - text.ptr));
    *out = NULL;
    *error = sag_sb_finish(&sb);
    return false;
  }
  if (text.len > SIZE_MAX - sizeof(struct sag_cstring_obj) - 1) {
    sag_panic("capacity overflow");
  }
  sag_cstring c = sag_alloc(sizeof(struct sag_cstring_obj) + text.len + 1);
  sag_count_init(&c->refcount, 1);
  if (text.len > 0) {
    memcpy(c->bytes, text.ptr, text.len);
  }
  c->bytes[text.len] = '\0';
  sag_count_inc(&sag_live_objects);
  *out = c;
  *error = sag_string_empty();
  return true;
}

void sag_cstring_retain(sag_cstring c) {
  if (c != NULL) {
    sag_count_inc(&c->refcount);
  }
}

void sag_cstring_release(sag_cstring c) {
  if (c != NULL && sag_count_dec(&c->refcount) == 0) {
    free(c);
    sag_count_dec(&sag_live_objects);
  }
}

const sag_c_char *sag_cstring_ptr(sag_cstring c) {
  return (const sag_c_char *)c->bytes;
}

static sag_string sag_string_from_bytes_lossy(const uint8_t *bytes, size_t len);

/* The length of the UTF-8 sequence at the start of `b`: the whole character when `*valid`, or
 * the maximal ill-formed prefix that one U+FFFD replaces (as Rust's `from_utf8_lossy`). */
static size_t sag_utf8_sequence(const uint8_t *b, size_t len, bool *valid) {
  uint8_t first = b[0];
  uint8_t low = 0x80;
  uint8_t high = 0xBF;
  size_t need;
  *valid = false;
  if (first < 0x80) {
    *valid = true;
    return 1;
  } else if (first >= 0xC2 && first <= 0xDF) {
    need = 1;
  } else if (first == 0xE0) {
    need = 2;
    low = 0xA0;
  } else if (first == 0xED) {
    need = 2;
    high = 0x9F;
  } else if (first >= 0xE1 && first <= 0xEF) {
    need = 2;
  } else if (first == 0xF0) {
    need = 3;
    low = 0x90;
  } else if (first == 0xF4) {
    need = 3;
    high = 0x8F;
  } else if (first >= 0xF1 && first <= 0xF3) {
    need = 3;
  } else {
    return 1;
  }
  for (size_t i = 1; i <= need; i++) {
    uint8_t min = i == 1 ? low : 0x80;
    uint8_t max = i == 1 ? high : 0xBF;
    if (i >= len || b[i] < min || b[i] > max) {
      return i;
    }
  }
  *valid = true;
  return need + 1;
}

sag_string sag_string_from_c_at(const sag_c_char *p, const char *file, int line) {
  if (p == NULL) {
    sag_panic_at("null pointer passed to string_from_c", file, line);
  }
  return sag_string_from_bytes_lossy((const uint8_t *)p, strlen((const char *)p));
}

/* A string with the UTF-8 in `bytes`, each ill-formed sequence replaced by U+FFFD. */
static sag_string sag_string_from_bytes_lossy(const uint8_t *bytes, size_t len) {
  sag_sb sb;
  sag_sb_init(&sb);
  size_t index = 0;
  while (index < len) {
    bool valid;
    size_t count = sag_utf8_sequence(bytes + index, len - index, &valid);
    if (valid) {
      sag_sb_push_bytes(&sb, (const char *)bytes + index, count);
    } else {
      sag_sb_push_bytes(&sb, "\xEF\xBF\xBD", 3);
    }
    index += count;
  }
  return sag_sb_finish(&sb);
}

/* ---- Standard library: std::process ------------------------------------------------------ */

#ifdef _WIN32
/* The kernel32 functions the runtime calls are declared here, because <windows.h> is not strict
 * C11. */
#if defined(_M_IX86) || defined(__i386__)
#define SAG_WINAPI __stdcall
#else
#define SAG_WINAPI
#endif
#endif

_Noreturn void sag_process_exit(int32_t code) {
  /* `_Exit` skips the `atexit` handlers, so leak checkers do not report the objects still
   * alive. */
  sag_end_begin();
  fflush(NULL);
  _Exit((int)code);
}

/* ---- Tasks (docs/spec/12-async.md) ----------------------------------------------------- */

typedef struct sag_task_timer_entry {
  uint64_t deadline;
  uint64_t order;
  sag_fnval resume;
} sag_task_timer_entry;

typedef struct sag_task_frame {
  sag_object *frame;
  void (*drop)(sag_object *);
} sag_task_frame;

/* The ready functions are `sag_ready[sag_ready_head..sag_ready_len]`. */
static SAG_THREAD_LOCAL sag_fnval *sag_ready = NULL;
static SAG_THREAD_LOCAL size_t sag_ready_head = 0;
static SAG_THREAD_LOCAL size_t sag_ready_len = 0;
static SAG_THREAD_LOCAL size_t sag_ready_cap = 0;
/* Sorted by deadline, then by order of creation. */
static SAG_THREAD_LOCAL sag_task_timer_entry *sag_timers = NULL;
static SAG_THREAD_LOCAL size_t sag_timers_len = 0;
static SAG_THREAD_LOCAL size_t sag_timers_cap = 0;
static SAG_THREAD_LOCAL uint64_t sag_timers_created = 0;
static SAG_THREAD_LOCAL sag_task_frame *sag_suspended = NULL;
static SAG_THREAD_LOCAL size_t sag_suspended_len = 0;
static SAG_THREAD_LOCAL size_t sag_suspended_cap = 0;
static SAG_THREAD_LOCAL size_t sag_task_depth = 0;
/* The jobs of `spawn_blocking` started by this thread and not yet back in its inbox. */
static SAG_THREAD_LOCAL size_t sag_task_jobs = 0;

#ifdef SAG_THREADS
static void sag_task_drain(void);
static void sag_task_wait_inbox(uint64_t nanos);
static void sag_task_close_inbox(void);
#endif

/* Room for one more element of `size` bytes in `*items`, which holds `len` of `*cap`. */
static void sag_task_grow(void **items, size_t *cap, size_t len, size_t size) {
  if (len < *cap) {
    return;
  }
  size_t grown = *cap == 0 ? 8 : *cap * 2;
  void *moved = realloc(*items, grown * size);
  if (moved == NULL) {
    sag_panic("out of memory");
  }
  *items = moved;
  *cap = grown;
}

void sag_task_schedule(sag_fnval resume) {
  if (sag_ready_head == sag_ready_len) {
    sag_ready_head = 0;
    sag_ready_len = 0;
  }
  sag_task_grow((void **)&sag_ready, &sag_ready_cap, sag_ready_len, sizeof(sag_fnval));
  sag_fnval_retain(resume);
  sag_ready[sag_ready_len++] = resume;
}

void sag_task_timer(uint64_t nanos, sag_fnval resume) {
  uint64_t now = sag_time_monotonic();
  uint64_t deadline = nanos > UINT64_MAX - now ? UINT64_MAX : now + nanos;
  sag_task_grow((void **)&sag_timers, &sag_timers_cap, sag_timers_len, sizeof(sag_task_timer_entry));
  size_t at = sag_timers_len;
  while (at > 0 && sag_timers[at - 1].deadline > deadline) {
    sag_timers[at] = sag_timers[at - 1];
    at--;
  }
  sag_fnval_retain(resume);
  sag_timers[at].deadline = deadline;
  sag_timers[at].order = sag_timers_created++;
  sag_timers[at].resume = resume;
  sag_timers_len++;
}

bool sag_task_step(void) {
#ifdef SAG_THREADS
  sag_task_drain();
#endif
  if (sag_ready_head < sag_ready_len) {
    sag_fnval resume = sag_ready[sag_ready_head++];
    ((void (*)(sag_object *))resume.fn)(resume.env);
    sag_fnval_release(resume);
    return true;
  }
  if (sag_timers_len == 0 && sag_task_jobs == 0) {
    return false;
  }
  uint64_t now = sag_time_monotonic();
  if (sag_timers_len == 0 || sag_timers[0].deadline > now) {
#ifdef SAG_THREADS
    /* A job may end before the next timer is due. */
    if (sag_task_jobs > 0) {
      sag_task_wait_inbox(sag_timers_len == 0 ? UINT64_MAX : sag_timers[0].deadline - now);
      return true;
    }
#endif
    sag_time_sleep(sag_timers[0].deadline - now);
    now = sag_time_monotonic();
  }
  /* The timers due, in order; the queue takes their references. */
  size_t due = 0;
  while (due < sag_timers_len && sag_timers[due].deadline <= now) {
    sag_task_schedule(sag_timers[due].resume);
    sag_fnval_release(sag_timers[due].resume);
    due++;
  }
  if (due == 0) {
    /* The clock went no further than the deadline: the first timer is due. */
    sag_task_schedule(sag_timers[0].resume);
    sag_fnval_release(sag_timers[0].resume);
    due = 1;
  }
  memmove(sag_timers, sag_timers + due, (sag_timers_len - due) * sizeof(sag_task_timer_entry));
  sag_timers_len -= due;
  return true;
}

void sag_task_enter(void) {
  sag_task_depth++;
}

void sag_task_leave(void) {
  sag_task_depth--;
}

bool sag_task_inside(void) {
  return sag_task_depth > 0;
}

void sag_task_suspended(sag_object *frame, void (*drop)(sag_object *)) {
  sag_task_grow((void **)&sag_suspended, &sag_suspended_cap, sag_suspended_len, sizeof(sag_task_frame));
  sag_suspended[sag_suspended_len].frame = frame;
  sag_suspended[sag_suspended_len].drop = drop;
  sag_suspended_len++;
}

void sag_task_resumed(sag_object *frame) {
  for (size_t i = sag_suspended_len; i > 0; i--) {
    if (sag_suspended[i - 1].frame == frame) {
      sag_suspended[i - 1] = sag_suspended[sag_suspended_len - 1];
      sag_suspended_len--;
      return;
    }
  }
}

void sag_task_shutdown(void) {
#ifdef SAG_THREADS
  sag_task_close_inbox();
#endif
  /* The suspended frames hold the tasks they await, whose waiters hold the frames: releasing
   * what each frame owns breaks those cycles. The frames are kept alive until all are done. */
  size_t frames = sag_suspended_len;
  for (size_t i = 0; i < frames; i++) {
    sag_object_retain(sag_suspended[i].frame);
  }
  for (size_t i = 0; i < frames; i++) {
    sag_suspended[i].drop(sag_suspended[i].frame);
  }
  for (size_t i = sag_ready_head; i < sag_ready_len; i++) {
    sag_fnval_release(sag_ready[i]);
  }
  for (size_t i = 0; i < sag_timers_len; i++) {
    sag_fnval_release(sag_timers[i].resume);
  }
  sag_task_frame *held = sag_suspended;
  sag_suspended = NULL;
  sag_suspended_len = 0;
  sag_suspended_cap = 0;
  for (size_t i = 0; i < frames; i++) {
    sag_object_release(held[i].frame);
  }
  free(held);
  free(sag_ready);
  free(sag_timers);
  sag_ready = NULL;
  sag_ready_head = 0;
  sag_ready_len = 0;
  sag_ready_cap = 0;
  sag_timers = NULL;
  sag_timers_len = 0;
  sag_timers_cap = 0;
}

/* ---- Standard library: std::thread ----------------------------------------------------- */

/* Threads still running: the leak check is only meaningful without them. */
static sag_count sag_threads_running;

#ifdef SAG_THREADS
/* A thread of `std::thread`, which `std::sys` starts and joins with the system's API
 * (docs/adr/0087-atomic-storage-and-std-sync.md, Q571). Every thread not yet joined is in the
 * list `sag_threads`, so that a thread nobody joins is not reported as leaked memory. */
typedef struct sag_thread {
  sag_fnval body;
  /* The system's thread, for the threads the core starts itself (`sag_core_thread_start`). */
#ifdef _WIN32
  void *handle;
#else
  pthread_t id;
#endif
  struct sag_thread *next;
} sag_thread;

static sag_thread *sag_threads = NULL;
static atomic_flag sag_threads_lock = ATOMIC_FLAG_INIT;

static void sag_threads_acquire(void) {
  while (atomic_flag_test_and_set_explicit(&sag_threads_lock, memory_order_acquire)) {
  }
}

static void sag_threads_release(void) {
  atomic_flag_clear_explicit(&sag_threads_lock, memory_order_release);
}

static void sag_static_locals_finish(void);

void *sag_thread_prepare(sag_fnval body) {
  sag_thread *thread = sag_alloc(sizeof(sag_thread));
  thread->body = body;
  sag_fnval_retain(body);
  sag_count_inc(&sag_threads_running);
  sag_threads_acquire();
  thread->next = sag_threads;
  sag_threads = thread;
  sag_threads_release();
  return thread;
}

void sag_thread_run(void *pointer) {
  sag_thread *thread = pointer;
  ((void (*)(sag_object *))thread->body.fn)(thread->body.env);
  sag_fnval_release(thread->body);
  sag_static_locals_finish();
  sag_task_shutdown();
  sag_drop_free();
  sag_count_dec(&sag_threads_running);
}

void sag_thread_finish(void *pointer) {
  sag_thread *thread = pointer;
  sag_threads_acquire();
  for (sag_thread **link = &sag_threads; *link != NULL; link = &(*link)->next) {
    if (*link == thread) {
      *link = thread->next;
      break;
    }
  }
  sag_threads_release();
  free(thread);
}

#ifdef _WIN32
extern void *SAG_WINAPI CreateThread(void *attributes, size_t stack_size, unsigned long(SAG_WINAPI *start)(void *), void *parameter, unsigned long flags,
                                     unsigned long *thread_id);
extern unsigned long SAG_WINAPI WaitForSingleObject(void *handle, unsigned long milliseconds);
extern int SAG_WINAPI CloseHandle(void *handle);

static unsigned long SAG_WINAPI sag_core_thread_main(void *thread) {
  sag_thread_run(thread);
  return 0;
}
#else
static void *sag_core_thread_main(void *thread) {
  sag_thread_run(thread);
  return NULL;
}
#endif

/* A thread of the core, for the jobs of `task::spawn_blocking`, which stays C with the executor
 * until S3; panics if the system cannot start one. */
static void *sag_core_thread_start(sag_fnval body) {
  sag_thread *thread = sag_thread_prepare(body);
#ifdef _WIN32
  thread->handle = CreateThread(NULL, 0, sag_core_thread_main, thread, 0, NULL);
  bool started = thread->handle != NULL;
#else
  bool started = pthread_create(&thread->id, NULL, sag_core_thread_main, thread) == 0;
#endif
  if (!started) {
    sag_panic("failed to spawn a thread");
  }
  return thread;
}

static void sag_core_thread_join(void *pointer) {
  sag_thread *thread = pointer;
#ifdef _WIN32
  WaitForSingleObject(thread->handle, 0xFFFFFFFFul);
  CloseHandle(thread->handle);
#else
  pthread_join(thread->id, NULL);
#endif
  sag_thread_finish(thread);
}

/* ---- Locks of the core ------------------------------------------------------------------- */

#ifdef _WIN32
extern void SAG_WINAPI InitializeSRWLock(void **lock);
extern void SAG_WINAPI AcquireSRWLockExclusive(void **lock);
extern void SAG_WINAPI ReleaseSRWLockExclusive(void **lock);
extern void SAG_WINAPI InitializeConditionVariable(void **condition);
extern int SAG_WINAPI SleepConditionVariableSRW(void **condition, void **lock, unsigned long milliseconds, unsigned long flags);
extern void SAG_WINAPI WakeAllConditionVariable(void **condition);

#define SAG_WIN_INFINITE 0xFFFFFFFFul
#endif

/* Its address identifies the running thread. */
static SAG_THREAD_LOCAL char sag_thread_marker;

/* ---- Tasks across threads (docs/adr/0073-spawn-blocking.md) ----------------------------- */

struct sag_task_job;

/* The inbox of an executor: the jobs of `spawn_blocking` whose body returned, in order. The
 * executor and the threads of its jobs hold references to it. */
typedef struct sag_task_inbox {
  sag_object header;
  struct sag_task_job *first;
  struct sag_task_job *last;
  /* The executor ended: jobs that end later only release what they hold. */
  bool closed;
#ifdef _WIN32
  void *lock;
  void *condition;
#else
  pthread_mutex_t lock;
  pthread_cond_t condition;
#endif
} sag_task_inbox;

/* A call of `spawn_blocking`: `body` runs on `thread`, then `done` on the executor of `inbox`. */
typedef struct sag_task_job {
  sag_object header;
  sag_fnval body;
  sag_fnval done;
  sag_task_inbox *inbox;
  void *thread;
  struct sag_task_job *next;
} sag_task_job;

static SAG_THREAD_LOCAL sag_task_inbox *sag_inbox = NULL;

static void sag_inbox_lock(sag_task_inbox *inbox) {
#ifdef _WIN32
  AcquireSRWLockExclusive(&inbox->lock);
#else
  pthread_mutex_lock(&inbox->lock);
#endif
}

static void sag_inbox_unlock(sag_task_inbox *inbox) {
#ifdef _WIN32
  ReleaseSRWLockExclusive(&inbox->lock);
#else
  pthread_mutex_unlock(&inbox->lock);
#endif
}

static void sag_task_inbox_drop(sag_object *object) {
#ifdef _WIN32
  (void)object;
#else
  pthread_cond_destroy(&((sag_task_inbox *)object)->condition);
  pthread_mutex_destroy(&((sag_task_inbox *)object)->lock);
#endif
}

static void sag_task_job_drop(sag_object *object) {
  sag_task_job *job = (sag_task_job *)object;
  sag_fnval_release(job->body);
  sag_fnval_release(job->done);
  sag_object_release(&job->inbox->header);
}

/* The inbox of this thread's executor, created when its first job starts. */
static sag_task_inbox *sag_task_inbox_get(void) {
  if (sag_inbox != NULL) {
    return sag_inbox;
  }
  sag_task_inbox *inbox = sag_object_alloc(sizeof(sag_task_inbox));
  inbox->first = NULL;
  inbox->last = NULL;
  inbox->closed = false;
#ifdef _WIN32
  InitializeSRWLock(&inbox->lock);
  InitializeConditionVariable(&inbox->condition);
#else
  if (pthread_mutex_init(&inbox->lock, NULL) != 0 || pthread_cond_init(&inbox->condition, NULL) != 0) {
    sag_panic("failed to create the inbox of the executor");
  }
#endif
  sag_object_init(&inbox->header, sag_task_inbox_drop, NULL);
  sag_inbox = inbox;
  return inbox;
}

/* On the job's thread: runs its body, then hands the job to the executor and wakes it. */
static void sag_task_job_run(sag_object *object) {
  sag_task_job *job = (sag_task_job *)object;
  sag_task_inbox *inbox = job->inbox;
  ((void (*)(sag_object *))job->body.fn)(job->body.env);
  sag_inbox_lock(inbox);
  bool closed = inbox->closed;
  if (!closed) {
    sag_object_retain(&job->header);
    job->next = NULL;
    if (inbox->last != NULL) {
      inbox->last->next = job;
    } else {
      inbox->first = job;
    }
    inbox->last = job;
#ifdef _WIN32
    WakeAllConditionVariable(&inbox->condition);
#else
    pthread_cond_broadcast(&inbox->condition);
#endif
  }
  sag_inbox_unlock(inbox);
#ifndef _WIN32
  /* The executor ended, so nobody will join this thread: it ends on its own, without being
   * reported as a leaked thread (ThreadSanitizer). On Windows its handle is never closed, which
   * only matters until the process ends. */
  if (closed) {
    pthread_detach(pthread_self());
  }
#endif
}

void sag_task_spawn_blocking(sag_fnval body, sag_fnval done) {
  sag_task_inbox *inbox = sag_task_inbox_get();
  sag_task_job *job = sag_object_alloc(sizeof(sag_task_job));
  job->body = body;
  job->done = done;
  job->inbox = inbox;
  job->thread = NULL;
  job->next = NULL;
  sag_fnval_retain(body);
  sag_fnval_retain(done);
  sag_object_retain(&inbox->header);
  sag_object_init(&job->header, sag_task_job_drop, NULL);
  sag_fnval run;
  run.fn = (sag_method)sag_task_job_run;
  run.env = &job->header;
  /* The thread holds its own reference while it runs. */
  job->thread = sag_core_thread_start(run);
  sag_object_release(&job->header);
  sag_task_jobs++;
}

/* Takes the jobs that ended: joins their threads and makes their `done` ready, in order. */
static void sag_task_drain(void) {
  sag_task_inbox *inbox = sag_inbox;
  if (inbox == NULL) {
    return;
  }
  sag_inbox_lock(inbox);
  sag_task_job *job = inbox->first;
  inbox->first = NULL;
  inbox->last = NULL;
  sag_inbox_unlock(inbox);
  while (job != NULL) {
    sag_task_job *next = job->next;
    sag_core_thread_join(job->thread);
    sag_task_jobs--;
    sag_task_schedule(job->done);
    sag_object_release(&job->header);
    job = next;
  }
}

/* Waits until a job ends, or at most `nanos` nanoseconds (UINT64_MAX: without limit). */
static void sag_task_wait_inbox(uint64_t nanos) {
  sag_task_inbox *inbox = sag_inbox;
  if (inbox == NULL) {
    return;
  }
  sag_inbox_lock(inbox);
  if (inbox->first == NULL) {
#ifdef _WIN32
    /* Whole milliseconds, rounded up, below INFINITE. */
    uint64_t millis = nanos == UINT64_MAX ? SAG_WIN_INFINITE : nanos / 1000000 + (nanos % 1000000 != 0);
    SleepConditionVariableSRW(&inbox->condition, &inbox->lock, millis >= SAG_WIN_INFINITE ? SAG_WIN_INFINITE : (unsigned long)millis, 0);
#else
    if (nanos == UINT64_MAX) {
      pthread_cond_wait(&inbox->condition, &inbox->lock);
    } else {
      struct timespec deadline;
      clock_gettime(CLOCK_REALTIME, &deadline);
      uint64_t seconds = nanos / 1000000000 + ((uint64_t)deadline.tv_nsec + nanos % 1000000000) / 1000000000;
      deadline.tv_nsec = (long)(((uint64_t)deadline.tv_nsec + nanos % 1000000000) % 1000000000);
      deadline.tv_sec = seconds > (uint64_t)INT32_MAX ? deadline.tv_sec + INT32_MAX : deadline.tv_sec + (time_t)seconds;
      pthread_cond_timedwait(&inbox->condition, &inbox->lock, &deadline);
    }
#endif
  }
  sag_inbox_unlock(inbox);
}

/* The executor ends: jobs that ended are joined and released without running their `done`, and
 * those still running will only release what they hold. */
static void sag_task_close_inbox(void) {
  sag_task_inbox *inbox = sag_inbox;
  if (inbox == NULL) {
    return;
  }
  sag_inbox = NULL;
  sag_inbox_lock(inbox);
  inbox->closed = true;
  sag_task_job *job = inbox->first;
  inbox->first = NULL;
  inbox->last = NULL;
  sag_inbox_unlock(inbox);
  while (job != NULL) {
    sag_task_job *next = job->next;
    sag_core_thread_join(job->thread);
    sag_object_release(&job->header);
    job = next;
  }
  sag_task_jobs = 0;
  sag_object_release(&inbox->header);
}
#endif

/* ---- Statics (docs/adr/0083-global-state.md) ------------------------------------------ */

/* The functions releasing the values of the statics computed, in order. */
static void (**sag_static_releases)(void) = NULL;
static size_t sag_static_releases_len = 0;
static size_t sag_static_releases_cap = 0;

#ifdef SAG_THREADS
/* One lock for the computation of every static, and the condition the threads waiting for one
 * wait on; zero-initialized on Windows. */
#ifdef _WIN32
static void *sag_statics_lock = NULL;
static void *sag_statics_condition = NULL;
#else
static pthread_mutex_t sag_statics_lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t sag_statics_condition = PTHREAD_COND_INITIALIZER;
#endif

static void sag_statics_acquire(void) {
#ifdef _WIN32
  AcquireSRWLockExclusive(&sag_statics_lock);
#else
  pthread_mutex_lock(&sag_statics_lock);
#endif
}

static void sag_statics_release_lock(void) {
#ifdef _WIN32
  ReleaseSRWLockExclusive(&sag_statics_lock);
#else
  pthread_mutex_unlock(&sag_statics_lock);
#endif
}
#endif

_Noreturn static void sag_static_cycle(const char *name, const char *file, int line) {
  char message[256];
  snprintf(message, sizeof message, "the static `%s` was used while its value was being computed", name);
  sag_panic_at(message, file, line);
}

bool sag_static_begin(sag_static_state *state, const char *name, const char *file, int line) {
#ifdef SAG_THREADS
  uintptr_t self = (uintptr_t)&sag_thread_marker;
  sag_statics_acquire();
  for (;;) {
    int stage = atomic_load(&state->stage);
    if (stage == 2) {
      sag_statics_release_lock();
      return false;
    }
    if (stage == 0) {
      atomic_store(&state->owner, self);
      atomic_store(&state->stage, 1);
      sag_statics_release_lock();
      return true;
    }
    if (atomic_load(&state->owner) == self) {
      sag_statics_release_lock();
      sag_static_cycle(name, file, line);
    }
#ifdef _WIN32
    SleepConditionVariableSRW(&sag_statics_condition, &sag_statics_lock, SAG_WIN_INFINITE, 0);
#else
    pthread_cond_wait(&sag_statics_condition, &sag_statics_lock);
#endif
  }
#else
  if (state->stage == 1) {
    sag_static_cycle(name, file, line);
  }
  state->stage = 1;
  return true;
#endif
}

void sag_static_end(sag_static_state *state, void (*release)(void)) {
#ifdef SAG_THREADS
  sag_statics_acquire();
#endif
  if (release != NULL) {
    if (sag_static_releases_len == sag_static_releases_cap) {
      size_t grown = sag_static_releases_cap == 0 ? 8 : sag_static_releases_cap * 2;
      void (**moved)(void) = realloc(sag_static_releases, grown * sizeof *moved);
      if (moved == NULL) {
        sag_panic("out of memory");
      }
      sag_static_releases = moved;
      sag_static_releases_cap = grown;
    }
    sag_static_releases[sag_static_releases_len++] = release;
  }
#ifdef SAG_THREADS
  atomic_store(&state->owner, 0);
  atomic_store_explicit(&state->stage, 2, memory_order_release);
#ifdef _WIN32
  WakeAllConditionVariable(&sag_statics_condition);
#else
  pthread_cond_broadcast(&sag_statics_condition);
#endif
  sag_statics_release_lock();
#else
  state->stage = 2;
#endif
}

/* The functions releasing the values of the `@thread_local` statics of this thread, in order. */
static SAG_THREAD_LOCAL void (**sag_static_locals)(void) = NULL;
static SAG_THREAD_LOCAL size_t sag_static_locals_len = 0;
static SAG_THREAD_LOCAL size_t sag_static_locals_cap = 0;

bool sag_static_local_begin(int *stage, const char *name, const char *file, int line) {
  if (*stage == 1) {
    sag_static_cycle(name, file, line);
  }
  *stage = 1;
  return true;
}

void sag_static_local_end(int *stage, void (*release)(void)) {
  if (release != NULL) {
    if (sag_static_locals_len == sag_static_locals_cap) {
      size_t grown = sag_static_locals_cap == 0 ? 8 : sag_static_locals_cap * 2;
      void (**moved)(void) = realloc(sag_static_locals, grown * sizeof *moved);
      if (moved == NULL) {
        sag_panic("out of memory");
      }
      sag_static_locals = moved;
      sag_static_locals_cap = grown;
    }
    sag_static_locals[sag_static_locals_len++] = release;
  }
  *stage = 2;
}

/* Releases the values of the `@thread_local` statics of the calling thread, the last computed
 * first, when it ends (Q529). */
static void sag_static_locals_finish(void) {
  while (sag_static_locals_len > 0) {
    sag_static_locals[--sag_static_locals_len]();
  }
  free(sag_static_locals);
  sag_static_locals = NULL;
  sag_static_locals_cap = 0;
}

/* Releases the values of the statics, the last computed first, so that the leak check does not
 * count them (Q532). Only once no other thread runs. */
static void sag_statics_finish(void) {
  while (sag_static_releases_len > 0) {
    sag_static_releases[--sag_static_releases_len]();
  }
  free(sag_static_releases);
  sag_static_releases = NULL;
  sag_static_releases_cap = 0;
}

/* ---- Program lifecycle ----------------------------------------------------------------- */

#ifdef _WIN32
/* The CRT's, from <io.h>: the standard streams are read and written byte for byte, as on the other
 * systems, instead of translating `\r\n` (docs/spec/11-standard-library.md#stdio). */
extern int _setmode(int fd, int mode);
#define SAG_O_BINARY 0x8000
#endif

void sag_rt_init(int argc, char **argv) {
#ifdef _WIN32
  _setmode(0, SAG_O_BINARY);
  _setmode(1, SAG_O_BINARY);
  _setmode(2, SAG_O_BINARY);
#endif
  sag_env_init(argc, (const char *const *)argv);
}

int sag_rt_finish(void) {
  if (fflush(stdout) != 0) {
    sag_panic("failed printing to stdout");
  }
  sag_task_shutdown();
  /* The main thread's `@thread_local` statics end with it. */
  sag_static_locals_finish();
  bool leak_check = getenv("SAG_LEAK_CHECK") != NULL;
  if (leak_check && sag_count_get(&sag_threads_running) == 0) {
    sag_statics_finish();
  }
  sag_drop_free();
  /* Objects of threads still running are alive. */
  size_t live = sag_count_get(&sag_live_objects);
  if (leak_check && live != 0 && sag_count_get(&sag_threads_running) == 0) {
    fprintf(stderr, "leak check: %zu ARC object(s) still alive at exit\n", live);
    return 102;
  }
  return 0;
}
