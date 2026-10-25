/* Calls the textcount library, built by Nova, from C. Part of the F3 acceptance program
 * (docs/roadmap.md). */
#include <inttypes.h>
#include <stdio.h>

#include "textcount.h"

int main(void) {
  const char *text = "one two\nthree\n";

  printf("%" PRIu64 " %" PRIu64 "\n", textcount_lines(text), textcount_words(text));
  return 0;
}
