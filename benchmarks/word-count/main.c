/* word-count: counts the words of a text and prints the most frequent. Strings and a hash table.
 * The text has the argument's number of words, chosen from 2000 by the generator MINSTD
 * (`x = x * 48271 % 2147483647`), the same in every language. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
  const char *word;
  uint64_t count;
} Entry;

/* An open-addressing table of words, which point into the text. */
typedef struct {
  Entry *slots;
  size_t cap;
  size_t len;
} Table;

static uint64_t hash(const char *word) {
  uint64_t h = 14695981039346656037ull;
  for (; *word != '\0'; word++) {
    h = (h ^ (unsigned char)*word) * 1099511628211ull;
  }
  return h;
}

static void insert(Table *table, const char *word);

static void grow(Table *table) {
  Entry *old = table->slots;
  size_t old_cap = table->cap;
  table->cap = old_cap == 0 ? 1024 : old_cap * 2;
  table->slots = calloc(table->cap, sizeof(Entry));
  table->len = 0;
  for (size_t i = 0; i < old_cap; i++) {
    if (old[i].word != NULL) {
      size_t slot = hash(old[i].word) & (table->cap - 1);
      while (table->slots[slot].word != NULL) {
        slot = (slot + 1) & (table->cap - 1);
      }
      table->slots[slot] = old[i];
      table->len++;
    }
  }
  free(old);
}

static void insert(Table *table, const char *word) {
  if ((table->len + 1) * 2 > table->cap) {
    grow(table);
  }
  size_t slot = hash(word) & (table->cap - 1);
  while (table->slots[slot].word != NULL) {
    if (strcmp(table->slots[slot].word, word) == 0) {
      table->slots[slot].count++;
      return;
    }
    slot = (slot + 1) & (table->cap - 1);
  }
  table->slots[slot].word = word;
  table->slots[slot].count = 1;
  table->len++;
}

/* The most frequent first, and words of the same count in the order of their bytes. */
static int by_count(const void *a, const void *b) {
  const Entry *x = a;
  const Entry *y = b;
  if (x->count != y->count) {
    return x->count < y->count ? 1 : -1;
  }
  return strcmp(x->word, y->word);
}

int main(int argc, char **argv) {
  uint64_t count = argc > 1 ? strtoull(argv[1], NULL, 10) : 1000;
  size_t cap = 1024, len = 0;
  char *text = malloc(cap);
  uint64_t seed = 42;
  for (uint64_t i = 0; i < count; i++) {
    char word[16];
    seed = seed * 48271 % 2147483647;
    int written = snprintf(word, sizeof word, "%sw%llu", i > 0 ? " " : "", (unsigned long long)(seed % 2000));
    if (len + (size_t)written + 1 > cap) {
      cap *= 2;
      text = realloc(text, cap);
    }
    memcpy(text + len, word, (size_t)written);
    len += (size_t)written;
  }
  text[len] = '\0';

  Table table = {NULL, 0, 0};
  char *start = text;
  for (size_t i = 0; i <= len; i++) {
    if (text[i] == ' ' || text[i] == '\0') {
      text[i] = '\0';
      insert(&table, start);
      start = text + i + 1;
    }
  }

  Entry *entries = malloc(table.len * sizeof(Entry));
  size_t found = 0;
  for (size_t i = 0; i < table.cap; i++) {
    if (table.slots[i].word != NULL) {
      entries[found++] = table.slots[i];
    }
  }
  qsort(entries, found, sizeof(Entry), by_count);

  printf("words %llu distinct %zu\n", (unsigned long long)count, found);
  for (size_t i = 0; i < 5 && i < found; i++) {
    printf("%s %llu\n", entries[i].word, (unsigned long long)entries[i].count);
  }
  free(entries);
  free(table.slots);
  free(text);
  return 0;
}
