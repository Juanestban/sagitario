/* `getenv` is standard C11; MSVC's CRT only deprecates it in favor of non-portable variants. */
#define _CRT_SECURE_NO_WARNINGS
/* The file system, clock and process functions of std are POSIX, not C11. */
#if !defined(_WIN32) && !defined(_POSIX_C_SOURCE)
#define _POSIX_C_SOURCE 200809L
#endif
/* Apple's headers hide `sysctlbyname` under strict POSIX. */
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE
#endif

#include "sagitario_runtime.h"

#include <errno.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef _WIN32
#include <dirent.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>
#ifdef SAG_THREADS
#include <pthread.h>
#endif
#ifdef __APPLE__
#include <sys/sysctl.h>
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

void sag_object_retain(void *pointer) {
  sag_object *object = pointer;
  if (object != NULL) {
    sag_count_inc(&object->strong);
  }
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

void sag_object_release(void *pointer) {
  sag_object *object = pointer;
  if (object == NULL || sag_count_dec(&object->strong) != 0) {
    return;
  }
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

void *sag_list_push(sag_object *object) {
  sag_list *list = (sag_list *)object;
  sag_list_grow(list);
  return list->data + list->len++ * list->size;
}

void *sag_list_insert(sag_object *object, size_t index, const char *file, int line) {
  sag_list *list = (sag_list *)object;
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
  if (list->len == 0) {
    sag_panic("pop from an empty list");
  }
  return list->data + --list->len * list->size;
}

void *sag_list_remove(sag_object *object, size_t index, const char *file, int line) {
  sag_list *list = (sag_list *)object;
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
  size_t len = list->len;
  /* Emptied first: releasing an element cannot reach the list again. */
  list->len = 0;
  if (list->release != NULL) {
    for (size_t i = 0; i < len; i++) {
      list->release(list->data + i * list->size);
    }
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
uint64_t sag_hash_u64(uint64_t value) {
  value += UINT64_C(0x9e3779b97f4a7c15);
  value = (value ^ (value >> 30)) * UINT64_C(0xbf58476d1ce4e5b9);
  value = (value ^ (value >> 27)) * UINT64_C(0x94d049bb133111eb);
  return value ^ (value >> 31);
}

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

static bool sag_utf8_valid(const uint8_t *bytes, size_t len) {
  size_t index = 0;
  while (index < len) {
    bool valid;
    index += sag_utf8_sequence(bytes + index, len - index, &valid);
    if (!valid) {
      return false;
    }
  }
  return true;
}

/* ---- Standard library: std::env, std::io, std::process --------------------------------- */

#ifdef _WIN32
#include <wchar.h>

/* Windows passes arguments and the environment as UTF-16, which the CRT's `argv` and `getenv`
 * convert to the ANSI code page. These kernel32 functions are declared here because
 * <windows.h> is not strict C11. */
#if defined(_M_IX86) || defined(__i386__)
#define SAG_WINAPI __stdcall
#else
#define SAG_WINAPI
#endif
extern wchar_t *SAG_WINAPI GetCommandLineW(void);
extern unsigned long SAG_WINAPI GetEnvironmentVariableW(const wchar_t *name, wchar_t *buffer, unsigned long size);

/* Appends UTF-16 code units as UTF-8; unpaired surrogates become U+FFFD. */
static void sag_sb_push_utf16(sag_sb *sb, const wchar_t *units, size_t len) {
  size_t index = 0;
  while (index < len) {
    uint32_t c = (uint32_t)units[index++];
    if (c >= 0xD800 && c <= 0xDBFF && index < len && (uint32_t)units[index] >= 0xDC00 && (uint32_t)units[index] <= 0xDFFF) {
      c = 0x10000 + ((c - 0xD800) << 10) + ((uint32_t)units[index++] - 0xDC00);
    } else if (c >= 0xD800 && c <= 0xDFFF) {
      c = 0xFFFD;
    }
    sag_sb_push_char(sb, c);
  }
}

/* The arguments as UTF-8, split from the command line with the rules of the C runtime. They
 * are plain bytes, not strings, so they are not counted as live objects. */
static sag_sb *sag_win_args = NULL;
static size_t sag_win_arg_count = 0;

static void sag_win_add_arg(const wchar_t *units, size_t len) {
  sag_sb *grown = realloc(sag_win_args, (sag_win_arg_count + 1) * sizeof(sag_sb));
  if (grown == NULL) {
    sag_panic("out of memory");
  }
  sag_win_args = grown;
  sag_sb_init(&sag_win_args[sag_win_arg_count]);
  sag_sb_push_utf16(&sag_win_args[sag_win_arg_count], units, len);
  sag_win_arg_count++;
}

static void sag_win_parse_args(void) {
  const wchar_t *p = GetCommandLineW();
  wchar_t *arg = sag_alloc((wcslen(p) + 1) * sizeof(wchar_t));
  size_t len = 0;
  /* The program name ends at the first space, or at the closing quote; backslashes are kept. */
  if (*p == L'"') {
    p++;
    while (*p != 0 && *p != L'"') {
      arg[len++] = *p++;
    }
    if (*p == L'"') {
      p++;
    }
  } else {
    while (*p != 0 && *p != L' ' && *p != L'\t') {
      arg[len++] = *p++;
    }
  }
  sag_win_add_arg(arg, len);
  for (;;) {
    while (*p == L' ' || *p == L'\t') {
      p++;
    }
    if (*p == 0) {
      break;
    }
    bool quoted = false;
    len = 0;
    while (*p != 0 && (quoted || (*p != L' ' && *p != L'\t'))) {
      if (*p == L'\\') {
        size_t slashes = 0;
        while (*p == L'\\') {
          slashes++;
          p++;
        }
        /* 2n backslashes and a quote: n backslashes, and the quote delimits. 2n + 1: n
         * backslashes and a literal quote. Backslashes elsewhere are literal. */
        size_t kept = *p == L'"' ? slashes / 2 : slashes;
        for (size_t i = 0; i < kept; i++) {
          arg[len++] = L'\\';
        }
        if (*p == L'"' && slashes % 2 == 1) {
          arg[len++] = *p++;
        }
      } else if (*p == L'"') {
        if (quoted && p[1] == L'"') {
          arg[len++] = L'"';
          p += 2;
        } else {
          quoted = !quoted;
          p++;
        }
      } else {
        arg[len++] = *p++;
      }
    }
    sag_win_add_arg(arg, len);
  }
  free(arg);
}
#else
static int sag_argc = 0;
static char **sag_argv = NULL;
#endif

size_t sag_env_arg_count(void) {
#ifdef _WIN32
  if (sag_win_args == NULL) {
    sag_win_parse_args();
  }
  return sag_win_arg_count;
#else
  return sag_argc > 0 ? (size_t)sag_argc : 0;
#endif
}

sag_string sag_env_arg(size_t index) {
  if (index >= sag_env_arg_count()) {
    return sag_string_empty();
  }
#ifdef _WIN32
  return sag_string_from_bytes_lossy(sag_win_args[index].data, sag_win_args[index].len);
#else
  return sag_string_from_bytes_lossy((const uint8_t *)sag_argv[index], strlen(sag_argv[index]));
#endif
}

sag_string sag_env_var(sag_string name, bool *found) {
  *found = false;
  if (name.len == 0 || memchr(name.ptr, '=', name.len) != NULL || memchr(name.ptr, 0, name.len) != NULL) {
    return sag_string_empty();
  }
#ifdef _WIN32
  wchar_t *wide = sag_alloc((name.len + 1) * sizeof(wchar_t));
  size_t len = 0;
  size_t index = 0;
  while (index < name.len) {
    uint32_t c = sag_utf8_next(name, &index);
    if (c >= 0x10000) {
      wide[len++] = (wchar_t)(0xD800 + ((c - 0x10000) >> 10));
      wide[len++] = (wchar_t)(0xDC00 + ((c - 0x10000) & 0x3FF));
    } else {
      wide[len++] = (wchar_t)c;
    }
  }
  wide[len] = 0;
  /* Without a buffer, the size needed including the terminator; 0 when the variable is unset. */
  unsigned long size = GetEnvironmentVariableW(wide, NULL, 0);
  sag_string result = sag_string_empty();
  if (size > 0) {
    wchar_t *value = sag_alloc((size_t)size * sizeof(wchar_t));
    unsigned long written = GetEnvironmentVariableW(wide, value, size);
    if (written < size) {
      sag_sb sb;
      sag_sb_init(&sb);
      sag_sb_push_utf16(&sb, value, written);
      result = sag_sb_finish(&sb);
      *found = true;
    }
    free(value);
  }
  free(wide);
  return result;
#else
  char *copy = sag_alloc(name.len + 1);
  memcpy(copy, name.ptr, name.len);
  copy[name.len] = 0;
  const char *value = getenv(copy);
  free(copy);
  if (value == NULL) {
    return sag_string_empty();
  }
  *found = true;
  return sag_string_from_bytes_lossy((const uint8_t *)value, strlen(value));
#endif
}

/* The code of the last failed C library call: its `errno`, or an unknown I/O error. */
static int32_t sag_io_errno(void) {
  return errno > 0 ? (int32_t)errno : SAG_IO_UNKNOWN;
}

int32_t sag_io_write(int32_t stream, sag_string text, bool newline) {
  FILE *file = stream == 2 ? stderr : stdout;
  errno = 0;
  if ((text.len > 0 && fwrite(text.ptr, 1, text.len, file) != text.len) || (newline && fputc('\n', file) == EOF)) {
    return sag_io_errno();
  }
  return 0;
}

int32_t sag_io_flush(void) {
  errno = 0;
  return fflush(stdout) == 0 ? 0 : sag_io_errno();
}

/* Makes the bytes read from stdin a string, or reports that they are not UTF-8. */
static sag_string sag_io_finish_read(sag_sb *sb, int32_t *status) {
  if (!sag_utf8_valid(sb->data, sb->len)) {
    free(sb->data);
    sag_sb_init(sb);
    *status = SAG_IO_INVALID_DATA;
    return sag_string_empty();
  }
  *status = 0;
  return sag_sb_finish(sb);
}

sag_string sag_io_read_line(int32_t *status) {
  sag_sb sb;
  sag_sb_init(&sb);
  bool any = false;
  int c;
  errno = 0;
  while ((c = getc(stdin)) != EOF) {
    any = true;
    if (c == '\n') {
      break;
    }
    char byte = (char)c;
    sag_sb_push_bytes(&sb, &byte, 1);
  }
  if (c == EOF && ferror(stdin)) {
    *status = sag_io_errno();
    clearerr(stdin);
    free(sb.data);
    return sag_string_empty();
  }
  if (!any) {
    *status = SAG_IO_END;
    return sag_string_empty();
  }
  if (sb.len > 0 && sb.data[sb.len - 1] == '\r') {
    sb.len--;
  }
  return sag_io_finish_read(&sb, status);
}

sag_string sag_io_read_all(int32_t *status) {
  sag_sb sb;
  sag_sb_init(&sb);
  char chunk[4096];
  size_t count;
  errno = 0;
  while ((count = fread(chunk, 1, sizeof chunk, stdin)) > 0) {
    sag_sb_push_bytes(&sb, chunk, count);
  }
  if (ferror(stdin)) {
    *status = sag_io_errno();
    clearerr(stdin);
    free(sb.data);
    return sag_string_empty();
  }
  return sag_io_finish_read(&sb, status);
}

/* A console on Windows, which has a console mode: the CRT's `_isatty` also says yes for `NUL` and
 * other character devices. `isatty` is from <unistd.h> elsewhere, which is not strict C11. */
#ifdef _WIN32
extern void *SAG_WINAPI GetStdHandle(unsigned long std_handle);
extern int SAG_WINAPI GetConsoleMode(void *console, unsigned long *mode);
#else
extern int isatty(int fd);
#endif

bool sag_io_is_terminal(int32_t stream) {
  if (stream < 0 || stream > 2) {
    return false;
  }
#ifdef _WIN32
  /* STD_INPUT_HANDLE, STD_OUTPUT_HANDLE and STD_ERROR_HANDLE are -10, -11 and -12. */
  unsigned long mode = 0;
  void *handle = GetStdHandle((unsigned long)(-10 - stream));
  return handle != NULL && handle != (void *)(intptr_t)-1 && GetConsoleMode(handle, &mode) != 0;
#else
  return isatty((int)stream) != 0;
#endif
}

/* Exactly `count` bytes of stdin; the bytes read are lost when the input ends first. */
sag_string sag_io_read_exact(size_t count, int32_t *status) {
  sag_sb sb;
  sag_sb_init(&sb);
  char chunk[4096];
  size_t left = count;
  errno = 0;
  while (left > 0) {
    size_t read = fread(chunk, 1, left < sizeof chunk ? left : sizeof chunk, stdin);
    if (read == 0) {
      break;
    }
    sag_sb_push_bytes(&sb, chunk, read);
    left -= read;
  }
  if (left > 0) {
    *status = ferror(stdin) ? sag_io_errno() : SAG_IO_UNEXPECTED_EOF;
    clearerr(stdin);
    free(sb.data);
    return sag_string_empty();
  }
  return sag_io_finish_read(&sb, status);
}

int32_t sag_io_error_kind(int32_t code) {
  if (code == SAG_IO_INVALID_DATA) {
    return SAG_IO_KIND_INVALID_DATA;
  }
  if (code == SAG_IO_UNEXPECTED_EOF) {
    return SAG_IO_KIND_UNEXPECTED_EOF;
  }
  /* Only EDOM, EILSEQ and ERANGE are standard C; the others exist on every supported system. */
#ifdef ENOENT
  if (code == ENOENT) {
    return SAG_IO_KIND_NOT_FOUND;
  }
#endif
#if defined(EACCES) && defined(EPERM)
  if (code == EACCES || code == EPERM) {
    return SAG_IO_KIND_PERMISSION_DENIED;
  }
#endif
#ifdef EEXIST
  if (code == EEXIST) {
    return SAG_IO_KIND_ALREADY_EXISTS;
  }
#endif
#ifdef EINVAL
  if (code == EINVAL) {
    return SAG_IO_KIND_INVALID_INPUT;
  }
#endif
  return SAG_IO_KIND_OTHER;
}

sag_string sag_io_error_message(int32_t code) {
  const char *message = "input/output error";
  if (code == SAG_IO_INVALID_DATA) {
    message = "stream did not contain valid UTF-8";
  } else if (code == SAG_IO_UNEXPECTED_EOF) {
    message = "failed to fill whole buffer";
  } else if (code > 0) {
    message = strerror((int)code);
  }
  return sag_string_from_bytes_lossy((const uint8_t *)message, strlen(message));
}

_Noreturn void sag_process_exit(int32_t code) {
  /* `_Exit` skips the `atexit` handlers, so leak checkers do not report the objects still
   * alive. */
  sag_end_begin();
  fflush(NULL);
  _Exit((int)code);
}

/* ---- Standard library: std::fs and std::path ------------------------------------------- */

#ifdef _WIN32
/* kernel32 declarations, with the Windows types spelled out: DWORD is `unsigned long`, BOOL
 * `int` and HANDLE `void *`. */
typedef struct sag_win_filetime {
  unsigned long low;
  unsigned long high;
} sag_win_filetime;

typedef struct sag_win_find_data {
  unsigned long attributes;
  sag_win_filetime created;
  sag_win_filetime accessed;
  sag_win_filetime written;
  unsigned long size_high;
  unsigned long size_low;
  unsigned long reserved0;
  unsigned long reserved1;
  wchar_t name[260];
  wchar_t alternate_name[14];
} sag_win_find_data;

extern void *SAG_WINAPI CreateFileW(const wchar_t *name, unsigned long access, unsigned long share, void *security, unsigned long disposition,
                                    unsigned long flags, void *template_file);
extern int SAG_WINAPI ReadFile(void *file, void *buffer, unsigned long size, unsigned long *read, void *overlapped);
extern int SAG_WINAPI WriteFile(void *file, const void *buffer, unsigned long size, unsigned long *written, void *overlapped);
extern int SAG_WINAPI CloseHandle(void *handle);
extern unsigned long SAG_WINAPI GetFileAttributesW(const wchar_t *name);
extern int SAG_WINAPI CreateDirectoryW(const wchar_t *name, void *security);
extern int SAG_WINAPI RemoveDirectoryW(const wchar_t *name);
extern int SAG_WINAPI DeleteFileW(const wchar_t *name);
extern int SAG_WINAPI MoveFileExW(const wchar_t *from, const wchar_t *to, unsigned long flags);
extern int SAG_WINAPI CopyFileW(const wchar_t *from, const wchar_t *to, int fail_if_exists);
extern void *SAG_WINAPI FindFirstFileW(const wchar_t *pattern, sag_win_find_data *data);
extern int SAG_WINAPI FindNextFileW(void *find, sag_win_find_data *data);
extern int SAG_WINAPI FindClose(void *find);
extern unsigned long SAG_WINAPI GetCurrentDirectoryW(unsigned long size, wchar_t *buffer);
extern unsigned long SAG_WINAPI GetLastError(void);

#define SAG_WIN_INVALID_HANDLE ((void *)(intptr_t)-1)
#define SAG_WIN_INVALID_ATTRIBUTES 0xFFFFFFFFul
#define SAG_WIN_ATTRIBUTE_DIRECTORY 0x10ul

/* The `errno` value of a Windows error code, so that kinds and messages match other systems. */
static int32_t sag_win_errno(unsigned long error) {
  switch (error) {
  case 2:   /* ERROR_FILE_NOT_FOUND */
  case 3:   /* ERROR_PATH_NOT_FOUND */
  case 15:  /* ERROR_INVALID_DRIVE */
  case 161: /* ERROR_BAD_PATHNAME */
    return ENOENT;
  case 5:  /* ERROR_ACCESS_DENIED */
  case 32: /* ERROR_SHARING_VIOLATION */
  case 33: /* ERROR_LOCK_VIOLATION */
    return EACCES;
  case 80:  /* ERROR_FILE_EXISTS */
  case 183: /* ERROR_ALREADY_EXISTS */
    return EEXIST;
  case 87:  /* ERROR_INVALID_PARAMETER */
  case 123: /* ERROR_INVALID_NAME */
    return EINVAL;
  case 145: /* ERROR_DIR_NOT_EMPTY */
    return ENOTEMPTY;
  case 267: /* ERROR_DIRECTORY */
    return ENOTDIR;
  case 8:  /* ERROR_NOT_ENOUGH_MEMORY */
  case 14: /* ERROR_OUTOFMEMORY */
    return ENOMEM;
  case 39:  /* ERROR_HANDLE_DISK_FULL */
  case 112: /* ERROR_DISK_FULL */
    return ENOSPC;
  default:
    return SAG_IO_UNKNOWN;
  }
}

static int32_t sag_win_last_errno(void) {
  return sag_win_errno(GetLastError());
}

/* `s` as a NUL-terminated UTF-16 string, or NULL if it contains a NUL character. */
static wchar_t *sag_wide(sag_string s) {
  if (memchr(s.ptr, 0, s.len) != NULL) {
    return NULL;
  }
  wchar_t *wide = sag_alloc((s.len + 1) * sizeof(wchar_t));
  size_t len = 0;
  size_t index = 0;
  while (index < s.len) {
    uint32_t c = sag_utf8_next(s, &index);
    if (c >= 0x10000) {
      wide[len++] = (wchar_t)(0xD800 + ((c - 0x10000) >> 10));
      wide[len++] = (wchar_t)(0xDC00 + ((c - 0x10000) & 0x3FF));
    } else {
      wide[len++] = (wchar_t)c;
    }
  }
  wide[len] = 0;
  return wide;
}
#else
/* `s` as a NUL-terminated C string, or NULL if it contains a NUL character. */
static char *sag_cpath(sag_string s) {
  if (memchr(s.ptr, 0, s.len) != NULL) {
    return NULL;
  }
  char *path = sag_alloc(s.len + 1);
  memcpy(path, s.ptr, s.len);
  path[s.len] = 0;
  return path;
}
#endif

int32_t sag_fs_kind(sag_string path) {
#ifdef _WIN32
  wchar_t *wide = sag_wide(path);
  if (wide == NULL) {
    return SAG_FS_MISSING;
  }
  unsigned long attributes = GetFileAttributesW(wide);
  free(wide);
  if (attributes == SAG_WIN_INVALID_ATTRIBUTES) {
    return SAG_FS_MISSING;
  }
  return (attributes & SAG_WIN_ATTRIBUTE_DIRECTORY) != 0 ? SAG_FS_DIR : SAG_FS_FILE;
#else
  char *cpath = sag_cpath(path);
  if (cpath == NULL) {
    return SAG_FS_MISSING;
  }
  struct stat info;
  int result = stat(cpath, &info);
  free(cpath);
  if (result != 0) {
    return SAG_FS_MISSING;
  }
  if (S_ISDIR(info.st_mode)) {
    return SAG_FS_DIR;
  }
  return S_ISREG(info.st_mode) ? SAG_FS_FILE : SAG_FS_OTHER;
#endif
}

/* Systems report a directory used as a file differently; they all become EISDIR. */
static int32_t sag_fs_file_error(sag_string path, int32_t code) {
  return code != 0 && sag_fs_kind(path) == SAG_FS_DIR ? EISDIR : code;
}

sag_string sag_fs_read(sag_string path, int32_t *status) {
  sag_sb sb;
  sag_sb_init(&sb);
#ifdef _WIN32
  wchar_t *wide = sag_wide(path);
  if (wide == NULL) {
    *status = EINVAL;
    return sag_string_empty();
  }
  /* GENERIC_READ, sharing everything, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL. */
  void *file = CreateFileW(wide, 0x80000000ul, 7ul, NULL, 3ul, 0x80ul, NULL);
  free(wide);
  if (file == SAG_WIN_INVALID_HANDLE) {
    *status = sag_fs_file_error(path, sag_win_last_errno());
    return sag_string_empty();
  }
  char chunk[4096];
  unsigned long count;
  int32_t code = 0;
  for (;;) {
    if (!ReadFile(file, chunk, sizeof chunk, &count, NULL)) {
      code = sag_win_last_errno();
      break;
    }
    if (count == 0) {
      break;
    }
    sag_sb_push_bytes(&sb, chunk, count);
  }
  CloseHandle(file);
#else
  char *cpath = sag_cpath(path);
  if (cpath == NULL) {
    *status = EINVAL;
    return sag_string_empty();
  }
  errno = 0;
  FILE *file = fopen(cpath, "rb");
  free(cpath);
  if (file == NULL) {
    *status = sag_fs_file_error(path, sag_io_errno());
    return sag_string_empty();
  }
  char chunk[4096];
  size_t count;
  int32_t code = 0;
  while ((count = fread(chunk, 1, sizeof chunk, file)) > 0) {
    sag_sb_push_bytes(&sb, chunk, count);
  }
  if (ferror(file)) {
    code = sag_fs_file_error(path, sag_io_errno());
  }
  fclose(file);
#endif
  if (code != 0) {
    free(sb.data);
    *status = code;
    return sag_string_empty();
  }
  return sag_io_finish_read(&sb, status);
}

int32_t sag_fs_write(sag_string path, sag_string text, bool append) {
#ifdef _WIN32
  wchar_t *wide = sag_wide(path);
  if (wide == NULL) {
    return EINVAL;
  }
  /* FILE_APPEND_DATA with OPEN_ALWAYS, or GENERIC_WRITE with CREATE_ALWAYS. */
  void *file = CreateFileW(wide, append ? 0x4ul : 0x40000000ul, 7ul, NULL, append ? 4ul : 2ul, 0x80ul, NULL);
  free(wide);
  if (file == SAG_WIN_INVALID_HANDLE) {
    return sag_fs_file_error(path, sag_win_last_errno());
  }
  int32_t code = 0;
  size_t done = 0;
  while (done < text.len) {
    size_t left = text.len - done;
    unsigned long size = left > 0x40000000u ? 0x40000000ul : (unsigned long)left;
    unsigned long written;
    if (!WriteFile(file, text.ptr + done, size, &written, NULL)) {
      code = sag_win_last_errno();
      break;
    }
    done += written;
  }
  if (!CloseHandle(file) && code == 0) {
    code = sag_win_last_errno();
  }
  return code;
#else
  char *cpath = sag_cpath(path);
  if (cpath == NULL) {
    return EINVAL;
  }
  errno = 0;
  FILE *file = fopen(cpath, append ? "ab" : "wb");
  free(cpath);
  if (file == NULL) {
    return sag_fs_file_error(path, sag_io_errno());
  }
  int32_t code = 0;
  if (text.len > 0 && fwrite(text.ptr, 1, text.len, file) != text.len) {
    code = sag_io_errno();
  }
  if (fclose(file) != 0 && code == 0) {
    code = sag_io_errno();
  }
  return code;
#endif
}

int32_t sag_fs_create_dir(sag_string path) {
#ifdef _WIN32
  wchar_t *wide = sag_wide(path);
  if (wide == NULL) {
    return EINVAL;
  }
  int32_t code = CreateDirectoryW(wide, NULL) ? 0 : sag_win_last_errno();
  free(wide);
  return code;
#else
  char *cpath = sag_cpath(path);
  if (cpath == NULL) {
    return EINVAL;
  }
  errno = 0;
  int32_t code = mkdir(cpath, 0777) == 0 ? 0 : sag_io_errno();
  free(cpath);
  return code;
#endif
}

int32_t sag_fs_remove_dir(sag_string path) {
#ifdef _WIN32
  wchar_t *wide = sag_wide(path);
  if (wide == NULL) {
    return EINVAL;
  }
  int32_t code = RemoveDirectoryW(wide) ? 0 : sag_win_last_errno();
  free(wide);
  return code;
#else
  char *cpath = sag_cpath(path);
  if (cpath == NULL) {
    return EINVAL;
  }
  errno = 0;
  int32_t code = rmdir(cpath) == 0 ? 0 : sag_io_errno();
  free(cpath);
  /* Some systems report a directory that is not empty as EEXIST. */
  return code == EEXIST ? ENOTEMPTY : code;
#endif
}

int32_t sag_fs_remove_file(sag_string path) {
#ifdef _WIN32
  wchar_t *wide = sag_wide(path);
  if (wide == NULL) {
    return EINVAL;
  }
  int32_t code = DeleteFileW(wide) ? 0 : sag_win_last_errno();
  free(wide);
#else
  char *cpath = sag_cpath(path);
  if (cpath == NULL) {
    return EINVAL;
  }
  errno = 0;
  int32_t code = unlink(cpath) == 0 ? 0 : sag_io_errno();
  free(cpath);
#endif
  return sag_fs_file_error(path, code);
}

int32_t sag_fs_rename(sag_string from, sag_string to) {
#ifdef _WIN32
  wchar_t *wide_from = sag_wide(from);
  wchar_t *wide_to = sag_wide(to);
  int32_t code = EINVAL;
  /* MOVEFILE_REPLACE_EXISTING | MOVEFILE_COPY_ALLOWED: replace the target, as POSIX does. */
  if (wide_from != NULL && wide_to != NULL) {
    code = MoveFileExW(wide_from, wide_to, 3ul) ? 0 : sag_win_last_errno();
  }
  free(wide_from);
  free(wide_to);
  return code;
#else
  char *cfrom = sag_cpath(from);
  char *cto = sag_cpath(to);
  int32_t code = EINVAL;
  if (cfrom != NULL && cto != NULL) {
    errno = 0;
    code = rename(cfrom, cto) == 0 ? 0 : sag_io_errno();
  }
  free(cfrom);
  free(cto);
  return code;
#endif
}

int32_t sag_fs_copy(sag_string from, sag_string to) {
  if (sag_fs_kind(from) == SAG_FS_DIR) {
    return EISDIR;
  }
#ifdef _WIN32
  wchar_t *wide_from = sag_wide(from);
  wchar_t *wide_to = sag_wide(to);
  int32_t code = EINVAL;
  if (wide_from != NULL && wide_to != NULL) {
    code = CopyFileW(wide_from, wide_to, 0) ? 0 : sag_win_last_errno();
  }
  free(wide_from);
  free(wide_to);
  return code;
#else
  char *cfrom = sag_cpath(from);
  char *cto = sag_cpath(to);
  if (cfrom == NULL || cto == NULL) {
    free(cfrom);
    free(cto);
    return EINVAL;
  }
  errno = 0;
  FILE *source = fopen(cfrom, "rb");
  free(cfrom);
  if (source == NULL) {
    free(cto);
    return sag_io_errno();
  }
  FILE *target = fopen(cto, "wb");
  free(cto);
  if (target == NULL) {
    int32_t code = sag_io_errno();
    fclose(source);
    return code;
  }
  int32_t code = 0;
  char chunk[4096];
  size_t count;
  while (code == 0 && (count = fread(chunk, 1, sizeof chunk, source)) > 0) {
    if (fwrite(chunk, 1, count, target) != count) {
      code = sag_io_errno();
    }
  }
  if (code == 0 && ferror(source)) {
    code = sag_io_errno();
  }
  fclose(source);
  if (fclose(target) != 0 && code == 0) {
    code = sag_io_errno();
  }
  return code;
#endif
}

sag_string sag_fs_read_dir(sag_string path, int32_t *status) {
  if (sag_fs_kind(path) == SAG_FS_FILE) {
    *status = ENOTDIR;
    return sag_string_empty();
  }
  sag_sb sb;
  sag_sb_init(&sb);
  bool first = true;
#ifdef _WIN32
  /* `path\*`, without doubling a trailing separator. */
  sag_sb pattern;
  sag_sb_init(&pattern);
  sag_sb_push_str(&pattern, path);
  if (path.len > 0 && path.ptr[path.len - 1] != '\\' && path.ptr[path.len - 1] != '/') {
    sag_sb_push_bytes(&pattern, "\\", 1);
  }
  sag_sb_push_bytes(&pattern, "*", 1);
  sag_string text = sag_sb_finish(&pattern);
  wchar_t *wide = path.len == 0 ? NULL : sag_wide(text);
  sag_string_release(text);
  if (wide == NULL) {
    *status = path.len == 0 ? ENOENT : EINVAL;
    return sag_string_empty();
  }
  sag_win_find_data data;
  void *find = FindFirstFileW(wide, &data);
  free(wide);
  if (find == SAG_WIN_INVALID_HANDLE) {
    *status = sag_win_last_errno();
    return sag_string_empty();
  }
  do {
    size_t len = wcslen(data.name);
    if ((len == 1 && data.name[0] == L'.') || (len == 2 && data.name[0] == L'.' && data.name[1] == L'.')) {
      continue;
    }
    if (!first) {
      sag_sb_push_bytes(&sb, "", 1); /* the NUL that separates names */
    }
    first = false;
    sag_sb_push_utf16(&sb, data.name, len);
  } while (FindNextFileW(find, &data));
  FindClose(find);
#else
  char *cpath = sag_cpath(path);
  if (cpath == NULL) {
    *status = EINVAL;
    return sag_string_empty();
  }
  errno = 0;
  DIR *dir = opendir(cpath);
  free(cpath);
  if (dir == NULL) {
    *status = sag_io_errno();
    return sag_string_empty();
  }
  struct dirent *entry;
  while ((entry = readdir(dir)) != NULL) {
    const char *name = entry->d_name;
    if (strcmp(name, ".") == 0 || strcmp(name, "..") == 0) {
      continue;
    }
    if (!first) {
      sag_sb_push_bytes(&sb, "", 1); /* the NUL that separates names */
    }
    first = false;
    sag_string text = sag_string_from_bytes_lossy((const uint8_t *)name, strlen(name));
    sag_sb_push_str(&sb, text);
    sag_string_release(text);
  }
  closedir(dir);
#endif
  *status = 0;
  return sag_sb_finish(&sb);
}

sag_string sag_fs_current_dir(int32_t *status) {
#ifdef _WIN32
  /* Without a buffer, the size needed including the terminator. */
  unsigned long size = GetCurrentDirectoryW(0, NULL);
  if (size == 0) {
    *status = sag_win_last_errno();
    return sag_string_empty();
  }
  wchar_t *buffer = sag_alloc((size_t)size * sizeof(wchar_t));
  unsigned long len = GetCurrentDirectoryW(size, buffer);
  if (len == 0 || len >= size) {
    free(buffer);
    *status = len == 0 ? sag_win_last_errno() : SAG_IO_UNKNOWN;
    return sag_string_empty();
  }
  sag_sb sb;
  sag_sb_init(&sb);
  sag_sb_push_utf16(&sb, buffer, len);
  free(buffer);
  *status = 0;
  return sag_sb_finish(&sb);
#else
  size_t size = 256;
  for (;;) {
    char *buffer = sag_alloc(size);
    errno = 0;
    if (getcwd(buffer, size) != NULL) {
      sag_string text = sag_string_from_bytes_lossy((const uint8_t *)buffer, strlen(buffer));
      free(buffer);
      *status = 0;
      return text;
    }
    free(buffer);
    if (errno != ERANGE || size > SIZE_MAX / 2) {
      *status = sag_io_errno();
      return sag_string_empty();
    }
    size *= 2;
  }
#endif
}

bool sag_path_windows(void) {
#ifdef _WIN32
  return true;
#else
  return false;
#endif
}

/* ---- Standard library: std::time ------------------------------------------------------- */

#ifdef _WIN32
extern int SAG_WINAPI QueryPerformanceCounter(int64_t *count);
extern int SAG_WINAPI QueryPerformanceFrequency(int64_t *frequency);
extern void SAG_WINAPI GetSystemTimePreciseAsFileTime(sag_win_filetime *time);
extern void SAG_WINAPI Sleep(unsigned long milliseconds);
#endif

uint64_t sag_time_monotonic(void) {
#ifdef _WIN32
  int64_t count = 0;
  int64_t frequency = 1;
  QueryPerformanceCounter(&count);
  QueryPerformanceFrequency(&frequency);
  if (frequency <= 0 || count < 0) {
    return 0;
  }
  uint64_t ticks = (uint64_t)count;
  uint64_t rate = (uint64_t)frequency;
  /* Split so that `ticks * 10^9` cannot overflow. */
  return ticks / rate * UINT64_C(1000000000) + ticks % rate * UINT64_C(1000000000) / rate;
#else
  struct timespec now;
  if (clock_gettime(CLOCK_MONOTONIC, &now) != 0 || now.tv_sec < 0) {
    return 0;
  }
  return (uint64_t)now.tv_sec * UINT64_C(1000000000) + (uint64_t)now.tv_nsec;
#endif
}

uint64_t sag_time_system(void) {
#ifdef _WIN32
  /* 100-nanosecond intervals since 1601-01-01; the Unix epoch is 11644473600 seconds later. */
  sag_win_filetime now;
  GetSystemTimePreciseAsFileTime(&now);
  uint64_t intervals = ((uint64_t)now.high << 32) | (uint64_t)now.low;
  uint64_t epoch = UINT64_C(116444736000000000);
  return intervals < epoch ? 0 : (intervals - epoch) * 100;
#else
  struct timespec now;
  if (clock_gettime(CLOCK_REALTIME, &now) != 0 || now.tv_sec < 0) {
    return 0;
  }
  return (uint64_t)now.tv_sec * UINT64_C(1000000000) + (uint64_t)now.tv_nsec;
#endif
}

void sag_time_sleep(uint64_t nanos) {
#ifdef _WIN32
  /* Whole milliseconds, rounded up so that the program never sleeps less than asked. */
  uint64_t millis = nanos / 1000000 + (nanos % 1000000 != 0);
  while (millis > 0) {
    unsigned long part = millis > 0x7FFFFFFFu ? 0x7FFFFFFFul : (unsigned long)millis;
    Sleep(part);
    millis -= part;
  }
#else
  struct timespec left;
  left.tv_sec = (time_t)(nanos / 1000000000);
  left.tv_nsec = (long)(nanos % 1000000000);
  while (nanosleep(&left, &left) != 0 && errno == EINTR) {
  }
#endif
}

/* ---- Standard library: std::process::Command ------------------------------------------- */

/* The parts of `text` separated by NUL characters, as NUL-terminated copies; `count` parts. */
static char **sag_split_nul(sag_string text, size_t count) {
  char **parts = sag_alloc((count + 1) * sizeof(char *));
  size_t start = 0;
  for (size_t index = 0; index < count; index++) {
    size_t end = start;
    while (end < text.len && text.ptr[end] != 0) {
      end++;
    }
    parts[index] = sag_alloc(end - start + 1);
    memcpy(parts[index], text.ptr + start, end - start);
    parts[index][end - start] = 0;
    start = end + 1;
  }
  parts[count] = NULL;
  return parts;
}

static void sag_free_parts(char **parts) {
  if (parts == NULL) {
    return;
  }
  for (size_t index = 0; parts[index] != NULL; index++) {
    free(parts[index]);
  }
  free(parts);
}

/* The result of running a child: stdout then stderr in one string, with the length of the
 * stdout part. Both halves are converted separately, so the split is a character boundary. */
static sag_string sag_process_result(sag_sb *out, sag_sb *err, size_t *split) {
  sag_string out_text = sag_string_from_bytes_lossy(out->data, out->len);
  sag_string err_text = sag_string_from_bytes_lossy(err->data, err->len);
  free(out->data);
  free(err->data);
  sag_sb sb;
  sag_sb_init(&sb);
  sag_sb_push_str(&sb, out_text);
  sag_sb_push_str(&sb, err_text);
  *split = out_text.len;
  sag_string_release(out_text);
  sag_string_release(err_text);
  return sag_sb_finish(&sb);
}

#ifdef _WIN32
typedef struct sag_win_security_attributes {
  unsigned long length;
  void *descriptor;
  int inherit;
} sag_win_security_attributes;

typedef struct sag_win_startup_info {
  unsigned long cb;
  wchar_t *reserved;
  wchar_t *desktop;
  wchar_t *title;
  unsigned long x;
  unsigned long y;
  unsigned long x_size;
  unsigned long y_size;
  unsigned long x_count_chars;
  unsigned long y_count_chars;
  unsigned long fill_attribute;
  unsigned long flags;
  unsigned short show_window;
  unsigned short reserved2_size;
  unsigned char *reserved2;
  void *std_input;
  void *std_output;
  void *std_error;
} sag_win_startup_info;

typedef struct sag_win_process_information {
  void *process;
  void *thread;
  unsigned long process_id;
  unsigned long thread_id;
} sag_win_process_information;

extern int SAG_WINAPI CreatePipe(void **read, void **write, sag_win_security_attributes *attributes, unsigned long size);
extern int SAG_WINAPI SetHandleInformation(void *handle, unsigned long mask, unsigned long flags);
extern void *SAG_WINAPI GetStdHandle(unsigned long which);
extern void *SAG_WINAPI GetCurrentProcess(void);
extern int SAG_WINAPI DuplicateHandle(void *source_process, void *source, void *target_process, void **target, unsigned long access, int inherit,
                                      unsigned long options);
extern int SAG_WINAPI CreateProcessW(const wchar_t *application, wchar_t *command_line, sag_win_security_attributes *process_attributes,
                                     sag_win_security_attributes *thread_attributes, int inherit_handles, unsigned long flags, void *environment,
                                     const wchar_t *directory, sag_win_startup_info *startup, sag_win_process_information *information);
extern void *SAG_WINAPI CreateThread(sag_win_security_attributes *attributes, size_t stack_size, unsigned long(SAG_WINAPI *start)(void *), void *parameter,
                                     unsigned long flags, unsigned long *thread_id);
extern unsigned long SAG_WINAPI WaitForSingleObject(void *handle, unsigned long milliseconds);
extern int SAG_WINAPI GetExitCodeProcess(void *process, unsigned long *code);
extern wchar_t *SAG_WINAPI GetEnvironmentStringsW(void);
extern int SAG_WINAPI FreeEnvironmentStringsW(wchar_t *block);

#define SAG_WIN_INFINITE 0xFFFFFFFFul

/* Appends `units` to a growing UTF-16 buffer. */
typedef struct sag_wbuf {
  wchar_t *data;
  size_t len;
  size_t cap;
} sag_wbuf;

static void sag_wbuf_push(sag_wbuf *buf, const wchar_t *units, size_t len) {
  if (buf->len + len + 1 > buf->cap) {
    size_t cap = buf->cap == 0 ? 256 : buf->cap;
    while (buf->len + len + 1 > cap) {
      cap *= 2;
    }
    wchar_t *grown = realloc(buf->data, cap * sizeof(wchar_t));
    if (grown == NULL) {
      sag_panic("out of memory");
    }
    buf->data = grown;
    buf->cap = cap;
  }
  memcpy(buf->data + buf->len, units, len * sizeof(wchar_t));
  buf->len += len;
  buf->data[buf->len] = 0;
}

static void sag_wbuf_push_unit(sag_wbuf *buf, wchar_t unit) {
  sag_wbuf_push(buf, &unit, 1);
}

/* Appends an argument quoted so that the C runtime's rules (sag_win_parse_args) read it back. */
static void sag_win_quote_arg(sag_wbuf *line, const wchar_t *arg) {
  size_t len = wcslen(arg);
  bool plain = len > 0;
  for (size_t i = 0; i < len; i++) {
    if (arg[i] == L' ' || arg[i] == L'\t' || arg[i] == L'"') {
      plain = false;
    }
  }
  if (plain) {
    sag_wbuf_push(line, arg, len);
    return;
  }
  sag_wbuf_push_unit(line, L'"');
  size_t slashes = 0;
  for (size_t i = 0; i < len; i++) {
    if (arg[i] == L'\\') {
      slashes++;
      continue;
    }
    /* Backslashes before a quote are doubled, and the quote escaped. */
    size_t count = arg[i] == L'"' ? slashes * 2 + 1 : slashes;
    for (size_t j = 0; j < count; j++) {
      sag_wbuf_push_unit(line, L'\\');
    }
    sag_wbuf_push_unit(line, arg[i]);
    slashes = 0;
  }
  /* Backslashes before the closing quote are doubled. */
  for (size_t j = 0; j < slashes * 2; j++) {
    sag_wbuf_push_unit(line, L'\\');
  }
  sag_wbuf_push_unit(line, L'"');
}

/* The UTF-16 copy of a NUL-terminated UTF-8 string. */
static wchar_t *sag_wide_c(const char *text) {
  return sag_wide(sag_string_from_static(text, strlen(text)));
}

/* Whether the `NAME=value` entry of an environment block has the name `name` (Windows names
 * ignore ASCII case). Entries starting with `=` are hidden variables of the shell. */
static bool sag_win_same_name(const wchar_t *entry, const wchar_t *name, size_t name_len) {
  for (size_t i = 0; i < name_len; i++) {
    wchar_t a = entry[i];
    wchar_t b = name[i];
    if (a >= L'a' && a <= L'z') {
      a = (wchar_t)(a - 32);
    }
    if (b >= L'a' && b <= L'z') {
      b = (wchar_t)(b - 32);
    }
    if (a != b || a == 0) {
      return false;
    }
  }
  return entry[name_len] == L'=';
}

/* The environment block of the child: the parent's, with the `NAME=value` entries in `vars`
 * replacing or adding variables. */
static wchar_t *sag_win_environment(char **vars) {
  sag_wbuf block = {NULL, 0, 0};
  wchar_t *parent = GetEnvironmentStringsW();
  for (const wchar_t *entry = parent; entry != NULL && *entry != 0; entry += wcslen(entry) + 1) {
    bool replaced = false;
    for (size_t i = 0; vars[i] != NULL && !replaced; i++) {
      wchar_t *var = sag_wide_c(vars[i]);
      size_t name_len = (size_t)(wcschr(var, L'=') - var);
      replaced = entry[0] != L'=' && sag_win_same_name(entry, var, name_len);
      free(var);
    }
    if (!replaced) {
      sag_wbuf_push(&block, entry, wcslen(entry) + 1);
    }
  }
  if (parent != NULL) {
    FreeEnvironmentStringsW(parent);
  }
  for (size_t i = 0; vars[i] != NULL; i++) {
    wchar_t *var = sag_wide_c(vars[i]);
    sag_wbuf_push(&block, var, wcslen(var) + 1);
    free(var);
  }
  /* The block ends with an empty entry. */
  sag_wbuf_push_unit(&block, 0);
  return block.data;
}

typedef struct sag_win_reader {
  void *pipe;
  sag_sb bytes;
} sag_win_reader;

static unsigned long SAG_WINAPI sag_win_read_pipe(void *parameter) {
  sag_win_reader *reader = parameter;
  char chunk[4096];
  unsigned long count;
  while (ReadFile(reader->pipe, chunk, sizeof chunk, &count, NULL) && count > 0) {
    sag_sb_push_bytes(&reader->bytes, chunk, count);
  }
  return 0;
}

/* An inheritable copy of one of the parent's standard handles. */
static void *sag_win_inherited(unsigned long which) {
  void *handle = GetStdHandle(which);
  void *copy = NULL;
  if (handle == NULL || handle == SAG_WIN_INVALID_HANDLE) {
    return NULL;
  }
  /* DUPLICATE_SAME_ACCESS */
  return DuplicateHandle(GetCurrentProcess(), handle, GetCurrentProcess(), &copy, 0, 1, 2ul) ? copy : NULL;
}
#else
/* Shared libraries on Apple systems cannot refer to `environ` directly. */
#ifdef __APPLE__
#include <crt_externs.h>
#define sag_environ (*_NSGetEnviron())
#else
extern char **environ;
#define sag_environ environ
#endif

/* Whether the `NAME=value` entry has the name `name`. */
static bool sag_same_name(const char *entry, const char *name, size_t name_len) {
  return strncmp(entry, name, name_len) == 0 && entry[name_len] == '=';
}

/* The environment of the child: the parent's, with the entries of `vars` replacing or adding
 * variables. Only the array is new; the strings are borrowed. */
static char **sag_environment(char **vars) {
  size_t count = 0;
  size_t extra = 0;
  while (sag_environ != NULL && sag_environ[count] != NULL) {
    count++;
  }
  while (vars[extra] != NULL) {
    extra++;
  }
  char **merged = sag_alloc((count + extra + 1) * sizeof(char *));
  size_t len = 0;
  for (size_t i = 0; i < count; i++) {
    bool replaced = false;
    for (size_t j = 0; j < extra && !replaced; j++) {
      replaced = sag_same_name(sag_environ[i], vars[j], (size_t)(strchr(vars[j], '=') - vars[j]));
    }
    if (!replaced) {
      merged[len++] = sag_environ[i];
    }
  }
  for (size_t j = 0; j < extra; j++) {
    merged[len++] = vars[j];
  }
  merged[len] = NULL;
  return merged;
}

/* The file `program` names: itself if it contains `/`, else the first executable file of that
 * name in the directories of PATH. NULL if there is none. */
static char *sag_find_program(const char *program) {
  if (strchr(program, '/') != NULL) {
    char *copy = sag_alloc(strlen(program) + 1);
    strcpy(copy, program);
    return copy;
  }
  const char *path = getenv("PATH");
  if (path == NULL) {
    path = "/usr/local/bin:/usr/bin:/bin";
  }
  size_t program_len = strlen(program);
  for (;;) {
    const char *end = strchr(path, ':');
    size_t dir_len = end == NULL ? strlen(path) : (size_t)(end - path);
    /* An empty directory means the current one. */
    char *candidate = sag_alloc(dir_len + program_len + 3);
    if (dir_len == 0) {
      memcpy(candidate, ".", 1);
      dir_len = 1;
    } else {
      memcpy(candidate, path, dir_len);
    }
    candidate[dir_len] = '/';
    memcpy(candidate + dir_len + 1, program, program_len + 1);
    struct stat info;
    if (stat(candidate, &info) == 0 && S_ISREG(info.st_mode) && access(candidate, X_OK) == 0) {
      return candidate;
    }
    free(candidate);
    if (end == NULL) {
      return NULL;
    }
    path = end + 1;
  }
}

static bool sag_cloexec_pipe(int fds[2]) {
  if (pipe(fds) != 0) {
    return false;
  }
  fcntl(fds[0], F_SETFD, FD_CLOEXEC);
  fcntl(fds[1], F_SETFD, FD_CLOEXEC);
  return true;
}

static void sag_close_fd(int *fd) {
  if (*fd >= 0) {
    close(*fd);
    *fd = -1;
  }
}

/* Reads what is available from `*fd` into `bytes`, closing it at the end of the stream. */
static void sag_drain_fd(int *fd, sag_sb *bytes) {
  char chunk[4096];
  ssize_t count = read(*fd, chunk, sizeof chunk);
  if (count > 0) {
    sag_sb_push_bytes(bytes, chunk, (size_t)count);
  } else if (count == 0 || errno != EINTR) {
    sag_close_fd(fd);
  }
}
#endif

sag_string sag_process_run(sag_string argv, size_t argc, sag_string env, size_t env_count, sag_string cwd, sag_string input, bool has_input,
                           bool capture, int32_t *results, size_t *split) {
  /* results: [0] an error code or 0, [1] the exit code, [2] whether there is one. */
  results[0] = 0;
  results[1] = 0;
  results[2] = 0;
  *split = 0;
  sag_sb out;
  sag_sb err;
  sag_sb_init(&out);
  sag_sb_init(&err);
  char **args = sag_split_nul(argv, argc);
  char **vars = sag_split_nul(env, env_count);
  /* The child shares stdout and stderr: what the program printed must come first. */
  fflush(NULL);
#ifdef _WIN32
  sag_wbuf line = {NULL, 0, 0};
  wchar_t *program = sag_wide_c(args[0]);
  /* The program name is only quoted, never escaped, when it holds spaces. */
  bool spaced = wcschr(program, L' ') != NULL || wcschr(program, L'\t') != NULL || *program == 0;
  if (spaced) {
    sag_wbuf_push_unit(&line, L'"');
  }
  sag_wbuf_push(&line, program, wcslen(program));
  if (spaced) {
    sag_wbuf_push_unit(&line, L'"');
  }
  free(program);
  for (size_t i = 1; i < argc; i++) {
    wchar_t *arg = sag_wide_c(args[i]);
    sag_wbuf_push_unit(&line, L' ');
    sag_win_quote_arg(&line, arg);
    free(arg);
  }
  wchar_t *environment = env_count > 0 ? sag_win_environment(vars) : NULL;
  wchar_t *directory = cwd.len > 0 ? sag_wide(cwd) : NULL;

  sag_win_security_attributes inheritable = {sizeof(sag_win_security_attributes), NULL, 1};
  void *in_read = NULL;
  void *in_write = NULL;
  sag_win_reader readers[2] = {{NULL, {NULL, 0, 0}}, {NULL, {NULL, 0, 0}}};
  void *child_out[2] = {NULL, NULL};
  sag_win_startup_info startup;
  memset(&startup, 0, sizeof startup);
  startup.cb = sizeof startup;
  startup.flags = 0x100ul; /* STARTF_USESTDHANDLES */
  /* HANDLE_FLAG_INHERIT is cleared on the parent's ends of the pipes. */
  if (has_input && CreatePipe(&in_read, &in_write, &inheritable, 0)) {
    SetHandleInformation(in_write, 1ul, 0ul);
    startup.std_input = in_read;
  } else {
    startup.std_input = sag_win_inherited((unsigned long)-10); /* STD_INPUT_HANDLE */
  }
  for (int i = 0; i < 2; i++) {
    if (capture && CreatePipe(&readers[i].pipe, &child_out[i], &inheritable, 0)) {
      SetHandleInformation(readers[i].pipe, 1ul, 0ul);
    } else {
      child_out[i] = sag_win_inherited(i == 0 ? (unsigned long)-11 : (unsigned long)-12); /* STD_OUTPUT_HANDLE, STD_ERROR_HANDLE */
    }
  }
  startup.std_output = child_out[0];
  startup.std_error = child_out[1];

  sag_win_process_information info;
  memset(&info, 0, sizeof info);
  /* CREATE_UNICODE_ENVIRONMENT */
  int created = CreateProcessW(NULL, line.data, NULL, NULL, 1, 0x400ul, environment, directory, &startup, &info);
  int32_t code = created ? 0 : sag_win_last_errno();
  /* The child holds its own copies of its ends. */
  if (startup.std_input != NULL) {
    CloseHandle(startup.std_input);
  }
  for (int i = 0; i < 2; i++) {
    if (child_out[i] != NULL) {
      CloseHandle(child_out[i]);
    }
  }
  if (created) {
    void *threads[2] = {NULL, NULL};
    for (int i = 0; i < 2; i++) {
      if (readers[i].pipe != NULL) {
        threads[i] = CreateThread(NULL, 0, sag_win_read_pipe, &readers[i], 0, NULL);
        if (threads[i] == NULL) {
          sag_win_read_pipe(&readers[i]);
        }
      }
    }
    if (in_write != NULL) {
      size_t done = 0;
      while (done < input.len) {
        size_t left = input.len - done;
        unsigned long written;
        /* A child that stops reading closes the pipe: the rest of the input is dropped. */
        if (!WriteFile(in_write, input.ptr + done, left > 65536 ? 65536ul : (unsigned long)left, &written, NULL)) {
          break;
        }
        done += written;
      }
      CloseHandle(in_write);
      in_write = NULL;
    }
    for (int i = 0; i < 2; i++) {
      if (threads[i] != NULL) {
        WaitForSingleObject(threads[i], SAG_WIN_INFINITE);
        CloseHandle(threads[i]);
      }
    }
    WaitForSingleObject(info.process, SAG_WIN_INFINITE);
    unsigned long exit_code = 0;
    GetExitCodeProcess(info.process, &exit_code);
    results[1] = (int32_t)exit_code;
    results[2] = 1;
    CloseHandle(info.process);
    CloseHandle(info.thread);
  }
  if (in_write != NULL) {
    CloseHandle(in_write);
  }
  for (int i = 0; i < 2; i++) {
    if (readers[i].pipe != NULL) {
      CloseHandle(readers[i].pipe);
    }
  }
  free(line.data);
  free(environment);
  free(directory);
  out = readers[0].bytes;
  err = readers[1].bytes;
#else
  int32_t code = 0;
  char *program = sag_find_program(args[0]);
  char *directory = cwd.len > 0 ? sag_cpath(cwd) : NULL;
  int in_fds[2] = {-1, -1};
  int out_fds[2] = {-1, -1};
  int err_fds[2] = {-1, -1};
  int exec_fds[2] = {-1, -1};
  if (program == NULL) {
    code = ENOENT;
  } else if ((has_input && !sag_cloexec_pipe(in_fds)) || (capture && (!sag_cloexec_pipe(out_fds) || !sag_cloexec_pipe(err_fds))) ||
             !sag_cloexec_pipe(exec_fds)) {
    code = sag_io_errno();
  }
  pid_t pid = -1;
  if (code == 0) {
    char **envp = env_count > 0 ? sag_environment(vars) : sag_environ;
    pid = fork();
    if (pid == 0) {
      /* The child: only async-signal-safe calls until `execve`. */
      if ((in_fds[0] >= 0 && dup2(in_fds[0], 0) < 0) || (out_fds[1] >= 0 && dup2(out_fds[1], 1) < 0) ||
          (err_fds[1] >= 0 && dup2(err_fds[1], 2) < 0) || (directory != NULL && chdir(directory) != 0)) {
        int error = errno;
        ssize_t ignored = write(exec_fds[1], &error, sizeof error);
        (void)ignored;
        _exit(127);
      }
      execve(program, args, envp);
      int error = errno;
      ssize_t ignored = write(exec_fds[1], &error, sizeof error);
      (void)ignored;
      _exit(127);
    }
    if (pid < 0) {
      code = sag_io_errno();
    }
    if (envp != sag_environ) {
      free(envp);
    }
  }
  sag_close_fd(&in_fds[0]);
  sag_close_fd(&out_fds[1]);
  sag_close_fd(&err_fds[1]);
  sag_close_fd(&exec_fds[1]);
  if (pid > 0) {
    /* The exec pipe closes when `execve` succeeds, or delivers the child's `errno`. */
    int error = 0;
    ssize_t count;
    do {
      count = read(exec_fds[0], &error, sizeof error);
    } while (count < 0 && errno == EINTR);
    if (count == (ssize_t)sizeof error) {
      code = error;
      sag_close_fd(&in_fds[1]);
    } else {
      /* A child that stops reading would raise SIGPIPE on the parent. */
      struct sigaction ignore;
      struct sigaction previous;
      memset(&ignore, 0, sizeof ignore);
      ignore.sa_handler = SIG_IGN;
      sigemptyset(&ignore.sa_mask);
      sigaction(SIGPIPE, &ignore, &previous);
      if (in_fds[1] >= 0) {
        fcntl(in_fds[1], F_SETFL, fcntl(in_fds[1], F_GETFL) | O_NONBLOCK);
      }
      size_t done = 0;
      if (in_fds[1] >= 0 && input.len == 0) {
        sag_close_fd(&in_fds[1]);
      }
      while (in_fds[1] >= 0 || out_fds[0] >= 0 || err_fds[0] >= 0) {
        struct pollfd fds[3];
        nfds_t count_fds = 0;
        int in_slot = -1;
        int out_slot = -1;
        int err_slot = -1;
        if (in_fds[1] >= 0) {
          in_slot = (int)count_fds;
          fds[count_fds].fd = in_fds[1];
          fds[count_fds++].events = POLLOUT;
        }
        if (out_fds[0] >= 0) {
          out_slot = (int)count_fds;
          fds[count_fds].fd = out_fds[0];
          fds[count_fds++].events = POLLIN;
        }
        if (err_fds[0] >= 0) {
          err_slot = (int)count_fds;
          fds[count_fds].fd = err_fds[0];
          fds[count_fds++].events = POLLIN;
        }
        for (nfds_t i = 0; i < count_fds; i++) {
          fds[i].revents = 0;
        }
        if (poll(fds, count_fds, -1) < 0) {
          if (errno == EINTR) {
            continue;
          }
          break;
        }
        if (in_slot >= 0 && fds[in_slot].revents != 0) {
          ssize_t written = write(in_fds[1], input.ptr + done, input.len - done);
          if (written > 0) {
            done += (size_t)written;
          }
          if (done == input.len || (written < 0 && errno != EAGAIN && errno != EINTR)) {
            sag_close_fd(&in_fds[1]);
          }
        }
        if (out_slot >= 0 && fds[out_slot].revents != 0) {
          sag_drain_fd(&out_fds[0], &out);
        }
        if (err_slot >= 0 && fds[err_slot].revents != 0) {
          sag_drain_fd(&err_fds[0], &err);
        }
      }
      sigaction(SIGPIPE, &previous, NULL);
    }
    int status = 0;
    while (waitpid(pid, &status, 0) < 0 && errno == EINTR) {
    }
    if (code == 0 && WIFEXITED(status)) {
      results[1] = WEXITSTATUS(status);
      results[2] = 1;
    }
  }
  sag_close_fd(&in_fds[1]);
  sag_close_fd(&out_fds[0]);
  sag_close_fd(&err_fds[0]);
  sag_close_fd(&exec_fds[0]);
  free(program);
  free(directory);
#endif
  sag_free_parts(args);
  sag_free_parts(vars);
  results[0] = code;
  if (code != 0) {
    free(out.data);
    free(err.data);
    return sag_string_empty();
  }
  return sag_process_result(&out, &err, split);
}

/* ---- Standard library: std::thread ----------------------------------------------------- */

/* Threads still running: the leak check is only meaningful without them. */
static sag_count sag_threads_running;

#ifdef SAG_THREADS
/* A thread started by `sag_thread_start`. Every thread not yet joined is in the list
 * `sag_threads`, so that a thread nobody joins is not reported as leaked memory. */
typedef struct sag_thread {
  sag_fnval body;
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

static void sag_thread_run(sag_thread *thread) {
  ((void (*)(sag_object *))thread->body.fn)(thread->body.env);
  sag_fnval_release(thread->body);
  sag_drop_free();
  sag_count_dec(&sag_threads_running);
}

#ifdef _WIN32
static unsigned long SAG_WINAPI sag_thread_main(void *thread) {
  sag_thread_run(thread);
  return 0;
}
#else
static void *sag_thread_main(void *thread) {
  sag_thread_run(thread);
  return NULL;
}
#endif

void *sag_thread_start(sag_fnval body) {
  sag_thread *thread = sag_alloc(sizeof(sag_thread));
  thread->body = body;
  sag_fnval_retain(body);
  sag_count_inc(&sag_threads_running);
  sag_threads_acquire();
  thread->next = sag_threads;
  sag_threads = thread;
  sag_threads_release();
#ifdef _WIN32
  thread->handle = CreateThread(NULL, 0, sag_thread_main, thread, 0, NULL);
  bool started = thread->handle != NULL;
#else
  bool started = pthread_create(&thread->id, NULL, sag_thread_main, thread) == 0;
#endif
  if (!started) {
    sag_panic("failed to spawn a thread");
  }
  return thread;
}

void sag_thread_join(void *pointer) {
  sag_thread *thread = pointer;
#ifdef _WIN32
  WaitForSingleObject(thread->handle, SAG_WIN_INFINITE);
  CloseHandle(thread->handle);
#else
  pthread_join(thread->id, NULL);
#endif
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
extern unsigned long SAG_WINAPI GetActiveProcessorCount(unsigned short group);
#endif

size_t sag_thread_available_parallelism(void) {
#if defined(_WIN32)
  /* All processor groups. */
  unsigned long count = GetActiveProcessorCount(0xFFFF);
#elif defined(__APPLE__)
  int count = 0;
  size_t size = sizeof count;
  if (sysctlbyname("hw.logicalcpu", &count, &size, NULL, 0) != 0) {
    count = 0;
  }
#else
  long count = sysconf(_SC_NPROCESSORS_ONLN);
#endif
  return count > 0 ? (size_t)count : 1;
}

/* ---- Standard library: std::sync ------------------------------------------------------- */

#ifdef _WIN32
extern void SAG_WINAPI InitializeSRWLock(void **lock);
extern void SAG_WINAPI AcquireSRWLockExclusive(void **lock);
extern void SAG_WINAPI ReleaseSRWLockExclusive(void **lock);
extern void SAG_WINAPI InitializeConditionVariable(void **condition);
extern int SAG_WINAPI SleepConditionVariableSRW(void **condition, void **lock, unsigned long milliseconds, unsigned long flags);
extern void SAG_WINAPI WakeAllConditionVariable(void **condition);
#endif

/* Its address identifies the running thread. */
static SAG_THREAD_LOCAL char sag_thread_marker;

static void sag_sync_handle_called(sag_object *object) {
  (void)object;
  sag_panic("internal error: a runtime handle of std::sync was called");
}

static sag_fnval sag_sync_handle(sag_object *object) {
  sag_fnval handle;
  handle.fn = (sag_method)sag_sync_handle_called;
  handle.env = object;
  return handle;
}

typedef struct sag_mutex {
  sag_object header;
  /* The address of the marker of the thread holding the lock, or 0. */
  atomic_uintptr_t owner;
#ifdef _WIN32
  void *lock;
  void *condition;
#else
  pthread_mutex_t lock;
  pthread_cond_t condition;
#endif
} sag_mutex;

static void sag_mutex_drop(sag_object *object) {
#ifdef _WIN32
  (void)object;
#else
  pthread_cond_destroy(&((sag_mutex *)object)->condition);
  pthread_mutex_destroy(&((sag_mutex *)object)->lock);
#endif
}

sag_fnval sag_sync_mutex_new(void) {
  sag_mutex *mutex = sag_object_alloc(sizeof(sag_mutex));
  atomic_init(&mutex->owner, 0);
#ifdef _WIN32
  InitializeSRWLock(&mutex->lock);
  InitializeConditionVariable(&mutex->condition);
#else
  if (pthread_mutex_init(&mutex->lock, NULL) != 0 || pthread_cond_init(&mutex->condition, NULL) != 0) {
    sag_panic("failed to create a mutex");
  }
#endif
  sag_object_init(&mutex->header, sag_mutex_drop, NULL);
  return sag_sync_handle(&mutex->header);
}

void sag_sync_mutex_lock(sag_fnval handle) {
  sag_mutex *mutex = (sag_mutex *)handle.env;
  uintptr_t self = (uintptr_t)&sag_thread_marker;
  if (atomic_load(&mutex->owner) == self) {
    sag_panic("a thread locked a `Mutex` it already holds: the function given to `update` must not use the same mutex");
  }
#ifdef _WIN32
  AcquireSRWLockExclusive(&mutex->lock);
#else
  pthread_mutex_lock(&mutex->lock);
#endif
  atomic_store(&mutex->owner, self);
}

void sag_sync_mutex_unlock(sag_fnval handle) {
  sag_mutex *mutex = (sag_mutex *)handle.env;
  atomic_store(&mutex->owner, 0);
#ifdef _WIN32
  ReleaseSRWLockExclusive(&mutex->lock);
#else
  pthread_mutex_unlock(&mutex->lock);
#endif
}

void sag_sync_mutex_wait(sag_fnval handle) {
  sag_mutex *mutex = (sag_mutex *)handle.env;
  atomic_store(&mutex->owner, 0);
#ifdef _WIN32
  SleepConditionVariableSRW(&mutex->condition, &mutex->lock, SAG_WIN_INFINITE, 0);
#else
  pthread_cond_wait(&mutex->condition, &mutex->lock);
#endif
  atomic_store(&mutex->owner, (uintptr_t)&sag_thread_marker);
}

void sag_sync_mutex_notify_all(sag_fnval handle) {
  sag_mutex *mutex = (sag_mutex *)handle.env;
#ifdef _WIN32
  WakeAllConditionVariable(&mutex->condition);
#else
  pthread_cond_broadcast(&mutex->condition);
#endif
}

typedef struct sag_atomic_cell {
  sag_object header;
  _Atomic uint64_t value;
} sag_atomic_cell;

static void sag_atomic_cell_drop(sag_object *object) {
  (void)object;
}

static _Atomic uint64_t *sag_atomic_value(sag_fnval handle) {
  return &((sag_atomic_cell *)handle.env)->value;
}

sag_fnval sag_sync_atomic_new(uint64_t value) {
  sag_atomic_cell *cell = sag_object_alloc(sizeof(sag_atomic_cell));
  atomic_init(&cell->value, value);
  sag_object_init(&cell->header, sag_atomic_cell_drop, NULL);
  return sag_sync_handle(&cell->header);
}

uint64_t sag_sync_atomic_load(sag_fnval cell) {
  return atomic_load(sag_atomic_value(cell));
}

void sag_sync_atomic_store(sag_fnval cell, uint64_t value) {
  atomic_store(sag_atomic_value(cell), value);
}

uint64_t sag_sync_atomic_swap(sag_fnval cell, uint64_t value) {
  return atomic_exchange(sag_atomic_value(cell), value);
}

/* `value` reduced to `bits` bits, then sign-extended or zero-extended back to 64. */
static uint64_t sag_atomic_canonical(uint64_t value, uint32_t bits, bool is_signed) {
  if (bits == 0) {
    bits = (uint32_t)(sizeof(size_t) * CHAR_BIT);
  }
  if (bits >= 64) {
    return value;
  }
  uint64_t mask = (UINT64_C(1) << bits) - 1;
  value &= mask;
  if (is_signed && (value >> (bits - 1)) != 0) {
    value |= ~mask;
  }
  return value;
}

uint64_t sag_sync_atomic_add(sag_fnval cell, uint64_t delta, bool subtract, uint32_t bits, bool is_signed) {
  _Atomic uint64_t *value = sag_atomic_value(cell);
  uint64_t previous = atomic_load(value);
  for (;;) {
    uint64_t next = sag_atomic_canonical(subtract ? previous - delta : previous + delta, bits, is_signed);
    if (atomic_compare_exchange_weak(value, &previous, next)) {
      return previous;
    }
  }
}

bool sag_sync_atomic_compare_exchange(sag_fnval cell, uint64_t expected, uint64_t desired, uint64_t *actual) {
  uint64_t current = expected;
  if (atomic_compare_exchange_strong(sag_atomic_value(cell), &current, desired)) {
    return true;
  }
  *actual = current;
  return false;
}
#endif

/* ---- Program lifecycle ----------------------------------------------------------------- */

#ifdef _WIN32
/* The CRT's, from <io.h>: the standard streams are read and written byte for byte, as on the other
 * systems, instead of translating `\r\n` (docs/spec/11-standard-library.md#stdio). */
extern int _setmode(int fd, int mode);
#define SAG_O_BINARY 0x8000
#endif

void sag_rt_init(int argc, char **argv) {
#ifdef _WIN32
  (void)argc;
  (void)argv;
  _setmode(0, SAG_O_BINARY);
  _setmode(1, SAG_O_BINARY);
  _setmode(2, SAG_O_BINARY);
#else
  sag_argc = argc;
  sag_argv = argv;
#endif
}

int sag_rt_finish(void) {
  if (fflush(stdout) != 0) {
    sag_panic("failed printing to stdout");
  }
  sag_drop_free();
  /* Objects of threads still running are alive. */
  size_t live = sag_count_get(&sag_live_objects);
  if (getenv("SAG_LEAK_CHECK") != NULL && live != 0 && sag_count_get(&sag_threads_running) == 0) {
    fprintf(stderr, "leak check: %zu ARC object(s) still alive at exit\n", live);
    return 102;
  }
  return 0;
}
