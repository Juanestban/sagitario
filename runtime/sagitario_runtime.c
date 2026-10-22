#include "sagitario_runtime.h"

#include <stdio.h>
#include <stdlib.h>

sag_string sag_string_from_static(const char *bytes, size_t len) {
  sag_string s;
  s.ptr = (const uint8_t *)bytes;
  s.len = len;
  s.buf = NULL;
  return s;
}

static void write_or_panic(FILE *stream, const uint8_t *bytes, size_t len) {
  if (len > 0 && fwrite(bytes, 1, len, stream) != len) {
    sag_panic(stream == stderr ? "failed printing to stderr" : "failed printing to stdout");
  }
}

void sag_print(sag_string s) {
  write_or_panic(stdout, s.ptr, s.len);
}

void sag_println(sag_string s) {
  write_or_panic(stdout, s.ptr, s.len);
  write_or_panic(stdout, (const uint8_t *)"\n", 1);
}

void sag_eprintln(sag_string s) {
  write_or_panic(stderr, s.ptr, s.len);
  write_or_panic(stderr, (const uint8_t *)"\n", 1);
}

_Noreturn void sag_panic(const char *message) {
  fflush(stdout);
  fprintf(stderr, "panic: %s\n", message);
  fflush(stderr);
  exit(101);
}

void sag_rt_init(int argc, char **argv) {
  (void)argc;
  (void)argv;
}

int sag_rt_finish(void) {
  if (fflush(stdout) != 0) {
    sag_panic("failed printing to stdout");
  }
  return 0;
}
