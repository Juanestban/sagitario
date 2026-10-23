/* `getenv` is standard C11; MSVC's CRT only deprecates it in favor of non-portable variants. */
#define _CRT_SECURE_NO_WARNINGS

#include "sagitario_runtime.h"

#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* ---- Panics ---------------------------------------------------------------------------- */

_Noreturn void sag_panic(const char *message) {
  fflush(stdout);
  fprintf(stderr, "panic: %s\n", message);
  fflush(stderr);
  exit(101);
}

_Noreturn void sag_panic_at(const char *message, const char *file, int line) {
  fflush(stdout);
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
  fflush(stdout);
  fprintf(stderr, "panic at %s:%d: %.*s\n", file, line, (int)message.len, (const char *)message.ptr);
  fflush(stderr);
  exit(101);
}

/* ---- Strings (ARC) --------------------------------------------------------------------- */

struct sag_string_buf {
  size_t refcount;
  size_t len;
  uint8_t bytes[];
};

/* Live ARC objects, checked at exit when SAG_LEAK_CHECK is set. Single-threaded MVP. */
static size_t sag_live_objects = 0;

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
    s.buf->refcount++;
  }
}

void sag_string_release(sag_string s) {
  if (s.buf != NULL && --s.buf->refcount == 0) {
    free(s.buf);
    sag_live_objects--;
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
  buf->refcount = 1;
  buf->len = sb->len;
  memcpy(buf->bytes, sb->data, sb->len);
  free(sb->data);
  sag_sb_init(sb);
  sag_live_objects++;

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
  size_t refcount;
  sag_sb sb;
};

sag_builder sag_builder_new(void) {
  sag_builder builder = sag_alloc(sizeof(struct sag_builder_obj));
  builder->refcount = 1;
  sag_sb_init(&builder->sb);
  sag_live_objects++;
  return builder;
}

void sag_builder_retain(sag_builder builder) {
  if (builder != NULL) {
    builder->refcount++;
  }
}

void sag_builder_release(sag_builder builder) {
  if (builder != NULL && --builder->refcount == 0) {
    free(builder->sb.data);
    free(builder);
    sag_live_objects--;
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
static sag_object **sag_drop_stack = NULL;
static size_t sag_drop_len = 0;
static size_t sag_drop_cap = 0;
static bool sag_dropping = false;

void *sag_object_alloc(size_t size) {
  return sag_alloc(size);
}

void sag_object_init(sag_object *object, void (*drop)(sag_object *object)) {
  object->strong = 1;
  object->weak = 0;
  object->drop = drop;
  sag_live_objects++;
}

void sag_object_retain(void *pointer) {
  sag_object *object = pointer;
  if (object != NULL) {
    object->strong++;
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
  if (object == NULL || --object->strong != 0) {
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
    if (next->weak == 0) {
      free(next);
      sag_live_objects--;
    }
  }
  sag_dropping = false;
}

/* ---- FFI --------------------------------------------------------------------------------- */

struct sag_cstring_obj {
  size_t refcount;
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
  c->refcount = 1;
  if (text.len > 0) {
    memcpy(c->bytes, text.ptr, text.len);
  }
  c->bytes[text.len] = '\0';
  sag_live_objects++;
  *out = c;
  *error = sag_string_empty();
  return true;
}

void sag_cstring_retain(sag_cstring c) {
  if (c != NULL) {
    c->refcount++;
  }
}

void sag_cstring_release(sag_cstring c) {
  if (c != NULL && --c->refcount == 0) {
    free(c);
    sag_live_objects--;
  }
}

const sag_c_char *sag_cstring_ptr(sag_cstring c) {
  return (const sag_c_char *)c->bytes;
}

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
  const uint8_t *bytes = (const uint8_t *)p;
  size_t len = strlen((const char *)p);
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

/* ---- Program lifecycle ----------------------------------------------------------------- */

void sag_rt_init(int argc, char **argv) {
  (void)argc;
  (void)argv;
}

int sag_rt_finish(void) {
  if (fflush(stdout) != 0) {
    sag_panic("failed printing to stdout");
  }
  free(sag_drop_stack);
  sag_drop_stack = NULL;
  sag_drop_cap = 0;
  if (getenv("SAG_LEAK_CHECK") != NULL && sag_live_objects != 0) {
    fprintf(stderr, "leak check: %zu ARC object(s) still alive at exit\n", sag_live_objects);
    return 102;
  }
  return 0;
}
