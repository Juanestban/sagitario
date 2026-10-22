/* Sagitario runtime: support code linked into every program. Strict, portable C11. */
#ifndef SAGITARIO_RUNTIME_H
#define SAGITARIO_RUNTIME_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* Heap storage of a reference-counted string (ARC strings arrive with String operations). */
typedef struct sag_string_buf sag_string_buf;

/* An immutable UTF-8 string. `buf` is NULL for static literals, which are never freed. */
typedef struct sag_string {
  const uint8_t *ptr;
  size_t len;
  sag_string_buf *buf;
} sag_string;

sag_string sag_string_from_static(const char *bytes, size_t len);

/* Prelude printing; panics if writing fails. */
void sag_print(sag_string s);
void sag_println(sag_string s);
void sag_eprintln(sag_string s);

/* Prints the message to stderr and exits with code 101. */
_Noreturn void sag_panic(const char *message);

void sag_rt_init(int argc, char **argv);
/* Flushes output and returns the process exit code for a normal exit. */
int sag_rt_finish(void);

#endif
