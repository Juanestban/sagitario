#ifndef FIXTURE_H
#define FIXTURE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* Macros: constants of every kind, also computed from others, and some that are not. */
#define FX_VERSION_MAJOR 1
#define FX_MASK 0xFFu
#define FX_SHIFTED (1u << 4)
#define FX_COMBINED (FX_MASK | FX_SHIFTED)
#define FX_CAST ((uint32_t)7)
#define FX_NEGATIVE_MACRO (-5)
#define FX_BIG 0x100000000ull
#define FX_RATIO 2.5
#define FX_SMALL .5e-3f
#define FX_NAME "fixture \"one\""
#define FX_LETTER 'x'
#define FX_TWICE(x) ((x) * 2)
#define FX_NOTHING
#define FX_ALIAS fx_window_create

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
typedef long (*fx_log_fn)(const char *message, long level);

typedef struct fx_events {
  fx_callback on_event;
  void *user;
  int (*filter)(const fx_rect *area);
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
long fx_log(fx_log_fn log, long level);

extern int fx_counter;
extern const char *const fx_version;

static inline int fx_twice(int x) { return 2 * x; }
long double fx_precise(void);

#endif
