/* map-count: counts how often each of 100000 keys comes, in a hash table of integers. The argument
 * is the number of keys drawn by the generator MINSTD (`x = x * 48271 % 2147483647`), the same in
 * every language. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

/* An open-addressing table; a slot is free while its count is zero. */
typedef struct {
  uint64_t *keys;
  uint64_t *counts;
  size_t cap;
  size_t len;
} Table;

static uint64_t mix(uint64_t key) {
  key ^= key >> 33;
  key *= 0xff51afd7ed558ccdull;
  key ^= key >> 33;
  return key;
}

static size_t find(const Table *table, uint64_t key) {
  size_t slot = mix(key) & (table->cap - 1);
  while (table->counts[slot] != 0 && table->keys[slot] != key) {
    slot = (slot + 1) & (table->cap - 1);
  }
  return slot;
}

static void grow(Table *table) {
  Table bigger = {NULL, NULL, table->cap == 0 ? 1024 : table->cap * 2, 0};
  bigger.keys = calloc(bigger.cap, sizeof(uint64_t));
  bigger.counts = calloc(bigger.cap, sizeof(uint64_t));
  for (size_t i = 0; i < table->cap; i++) {
    if (table->counts[i] != 0) {
      size_t slot = find(&bigger, table->keys[i]);
      bigger.keys[slot] = table->keys[i];
      bigger.counts[slot] = table->counts[i];
      bigger.len++;
    }
  }
  free(table->keys);
  free(table->counts);
  *table = bigger;
}

static void add(Table *table, uint64_t key) {
  if ((table->len + 1) * 2 > table->cap) {
    grow(table);
  }
  size_t slot = find(table, key);
  if (table->counts[slot] == 0) {
    table->keys[slot] = key;
    table->len++;
  }
  table->counts[slot]++;
}

static uint64_t get(const Table *table, uint64_t key) {
  return table->counts[find(table, key)];
}

int main(int argc, char **argv) {
  uint64_t count = argc > 1 ? strtoull(argv[1], NULL, 10) : 1000;
  Table table = {NULL, NULL, 0, 0};
  uint64_t seed = 42;
  for (uint64_t i = 0; i < count; i++) {
    seed = seed * 48271 % 2147483647;
    add(&table, seed % 100000);
  }

  /* The largest count, at its smallest key. */
  uint64_t most = 0, most_key = 0;
  for (size_t i = 0; i < table.cap; i++) {
    if (table.counts[i] != 0 && (table.counts[i] > most || (table.counts[i] == most && table.keys[i] < most_key))) {
      most = table.counts[i];
      most_key = table.keys[i];
    }
  }

  uint64_t small = 0;
  for (uint64_t key = 0; key < 100; key++) {
    small += get(&table, key);
  }

  printf("keys %zu most %llu at %llu small %llu\n", table.len, (unsigned long long)most, (unsigned long long)most_key, (unsigned long long)small);
  free(table.keys);
  free(table.counts);
  return 0;
}
