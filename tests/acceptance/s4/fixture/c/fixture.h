#ifndef FIXTURE_H
#define FIXTURE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* An opaque type, whose definition is in fixture.c. */
typedef struct fx_window fx_window;

typedef enum fx_flags {
  FX_NONE = 0,
  FX_VISIBLE = 1 << 0,
  FX_RESIZABLE = 1 << 1,
  FX_ALL = FX_VISIBLE | FX_RESIZABLE
} fx_flags;

enum { FX_LIMIT = 16, FX_NEGATIVE = -3 };

typedef struct {
  int32_t x, y;
  uint32_t w, h;
} fx_rect;

typedef struct fx_point {
  float x;
  double y;
} fx_point;

typedef union fx_value {
  int64_t integer;
  double real;
  const char *text;
} fx_value;

typedef void (*fx_callback)(void *user, int code);

typedef struct fx_events {
  fx_callback on_event;
  void *user;
  int (*filter)(const void *area);
  struct {
    int count;
    char name[FX_LIMIT / 2];
  } stats;
} fx_events;

struct fx_bits {
  unsigned a : 3;
  unsigned b : 5;
};

fx_window *fx_window_create(const char *title, fx_rect area, fx_flags flags);
void fx_window_destroy(fx_window *window);
uint32_t fx_window_area(const fx_window *window);
bool fx_rect_contains(const fx_rect *rect, fx_point point);
int fx_sum(int count, ...);
size_t fx_copy_title(char *buffer, size_t size, const fx_window *window);
void fx_events_fire(fx_events *events, int code);
fx_value fx_value_of_int(int64_t value);
int fx_bits_total(struct fx_bits *bits);
int fx_match(int type);

extern int fx_counter;
extern const char *const fx_version;

static inline int fx_twice(int x) { return 2 * x; }
long double fx_precise(void);

#endif
