/* channels: a producer thread sends `0..n` through a channel and the main thread receives and adds
 * them. A queue behind a lock and a condition: Win32 on Windows, POSIX threads elsewhere. The
 * argument is `n`. */
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#ifdef _WIN32
#include <windows.h>
typedef SRWLOCK Lock;
typedef CONDITION_VARIABLE Condition;
#define LOCK_INIT(l) InitializeSRWLock(l)
#define CONDITION_INIT(c) InitializeConditionVariable(c)
#define LOCK(l) AcquireSRWLockExclusive(l)
#define UNLOCK(l) ReleaseSRWLockExclusive(l)
#define WAIT(c, l) SleepConditionVariableSRW(c, l, INFINITE, 0)
#define NOTIFY(c) WakeAllConditionVariable(c)
#else
#include <pthread.h>
typedef pthread_mutex_t Lock;
typedef pthread_cond_t Condition;
#define LOCK_INIT(l) pthread_mutex_init(l, NULL)
#define CONDITION_INIT(c) pthread_cond_init(c, NULL)
#define LOCK(l) pthread_mutex_lock(l)
#define UNLOCK(l) pthread_mutex_unlock(l)
#define WAIT(c, l) pthread_cond_wait(c, l)
#define NOTIFY(c) pthread_cond_broadcast(c)
#endif

/* An unbounded queue: a growing ring of values. */
typedef struct {
  Lock lock;
  Condition ready;
  uint64_t *values;
  size_t head;
  size_t len;
  size_t cap;
  bool closed;
} Channel;

static void channel_send(Channel *channel, uint64_t value) {
  LOCK(&channel->lock);
  if (channel->len == channel->cap) {
    size_t cap = channel->cap == 0 ? 1024 : channel->cap * 2;
    uint64_t *grown = malloc(cap * sizeof(uint64_t));
    for (size_t i = 0; i < channel->len; i++) {
      grown[i] = channel->values[(channel->head + i) % channel->cap];
    }
    free(channel->values);
    channel->values = grown;
    channel->head = 0;
    channel->cap = cap;
  }
  channel->values[(channel->head + channel->len) % channel->cap] = value;
  channel->len++;
  NOTIFY(&channel->ready);
  UNLOCK(&channel->lock);
}

static bool channel_receive(Channel *channel, uint64_t *value) {
  LOCK(&channel->lock);
  while (channel->len == 0 && !channel->closed) {
    WAIT(&channel->ready, &channel->lock);
  }
  bool got = channel->len > 0;
  if (got) {
    *value = channel->values[channel->head];
    channel->head = (channel->head + 1) % channel->cap;
    channel->len--;
  }
  UNLOCK(&channel->lock);
  return got;
}

static void close_channel(Channel *channel) {
  LOCK(&channel->lock);
  channel->closed = true;
  NOTIFY(&channel->ready);
  UNLOCK(&channel->lock);
}

typedef struct {
  Channel *channel;
  uint64_t n;
} Producer;

static void produce(Producer *producer) {
  for (uint64_t i = 0; i < producer->n; i++) {
    channel_send(producer->channel, i);
  }
  close_channel(producer->channel);
}

#ifdef _WIN32
static DWORD WINAPI entry(LPVOID producer) {
  produce(producer);
  return 0;
}
#else
static void *entry(void *producer) {
  produce(producer);
  return NULL;
}
#endif

int main(int argc, char **argv) {
  uint64_t n = argc > 1 ? strtoull(argv[1], NULL, 10) : 1000;
  Channel channel = {0};
  LOCK_INIT(&channel.lock);
  CONDITION_INIT(&channel.ready);
  Producer producer = {&channel, n};

#ifdef _WIN32
  HANDLE thread = CreateThread(NULL, 0, entry, &producer, 0, NULL);
#else
  pthread_t thread;
  pthread_create(&thread, NULL, entry, &producer);
#endif

  uint64_t count = 0, sum = 0, value;
  while (channel_receive(&channel, &value)) {
    count++;
    sum += value;
  }

#ifdef _WIN32
  WaitForSingleObject(thread, INFINITE);
  CloseHandle(thread);
#else
  pthread_join(thread, NULL);
#endif
  printf("received %llu sum %llu\n", (unsigned long long)count, (unsigned long long)sum);
  free(channel.values);
  return 0;
}
