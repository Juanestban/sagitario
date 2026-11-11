/* sort: sorts the argument's number of integers from the generator MINSTD
 * (`x = x * 48271 % 2147483647`), the same in every language, with the standard sort of each. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static int ascending(const void *a, const void *b) {
  uint64_t x = *(const uint64_t *)a;
  uint64_t y = *(const uint64_t *)b;
  return (x > y) - (x < y);
}

int main(int argc, char **argv) {
  size_t count = argc > 1 ? (size_t)strtoull(argv[1], NULL, 10) : 1000;
  uint64_t *values = malloc(count * sizeof(uint64_t));
  if (values == NULL) {
    return 1;
  }
  uint64_t seed = 42;
  for (size_t i = 0; i < count; i++) {
    seed = seed * 48271 % 2147483647;
    values[i] = seed;
  }

  qsort(values, count, sizeof(uint64_t), ascending);

  uint64_t checksum = 0;
  for (size_t i = 0; i < count; i += 1000) {
    checksum += values[i];
  }

  printf("min %llu max %llu checksum %llu\n", (unsigned long long)values[0], (unsigned long long)values[count - 1], (unsigned long long)checksum);
  free(values);
  return 0;
}
