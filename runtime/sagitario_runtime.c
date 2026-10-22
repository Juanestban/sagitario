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

/* ---- Program lifecycle ----------------------------------------------------------------- */

void sag_rt_init(int argc, char **argv) {
  (void)argc;
  (void)argv;
}

int sag_rt_finish(void) {
  if (fflush(stdout) != 0) {
    sag_panic("failed printing to stdout");
  }
  if (getenv("SAG_LEAK_CHECK") != NULL && sag_live_objects != 0) {
    fprintf(stderr, "leak check: %zu ARC object(s) still alive at exit\n", sag_live_objects);
    return 102;
  }
  return 0;
}
