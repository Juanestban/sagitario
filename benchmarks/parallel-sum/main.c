/* parallel-sum: four threads add `i * i % 1000` over their quarter of `0..n`, and the main thread
 * adds their results. Threads and joins: Win32 threads on Windows, POSIX threads elsewhere. The
 * argument is `n`. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#ifdef _WIN32
#include <windows.h>
#else
#include <pthread.h>
#endif

#define THREADS 4

typedef struct {
  uint64_t start;
  uint64_t end;
  uint64_t sum;
} Part;

static void partial(Part *part) {
  uint64_t sum = 0;
  for (uint64_t i = part->start; i < part->end; i++) {
    sum += i * i % 1000;
  }
  part->sum = sum;
}

#ifdef _WIN32
static DWORD WINAPI entry(LPVOID part) {
  partial(part);
  return 0;
}
#else
static void *entry(void *part) {
  partial(part);
  return NULL;
}
#endif

int main(int argc, char **argv) {
  uint64_t n = argc > 1 ? strtoull(argv[1], NULL, 10) : 1000;
  uint64_t chunk = n / THREADS;
  Part parts[THREADS];
#ifdef _WIN32
  HANDLE threads[THREADS];
#else
  pthread_t threads[THREADS];
#endif

  for (int index = 0; index < THREADS; index++) {
    parts[index].start = (uint64_t)index * chunk;
    parts[index].end = index == THREADS - 1 ? n : parts[index].start + chunk;
#ifdef _WIN32
    threads[index] = CreateThread(NULL, 0, entry, &parts[index], 0, NULL);
#else
    pthread_create(&threads[index], NULL, entry, &parts[index]);
#endif
  }

  uint64_t total = 0;
  for (int index = 0; index < THREADS; index++) {
#ifdef _WIN32
    WaitForSingleObject(threads[index], INFINITE);
    CloseHandle(threads[index]);
#else
    pthread_join(threads[index], NULL);
#endif
    total += parts[index].sum;
  }

  printf("%llu\n", (unsigned long long)total);
  return 0;
}
