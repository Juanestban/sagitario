#include "fixture.h"
#include <stdarg.h>
#include <stdlib.h>
#include <string.h>

struct fx_window { char title[32]; fx_rect area; fx_flags flags; };

int fx_counter = 0;
const char *const fx_version = "1.2.3";

fx_window *fx_window_create(const char *title, fx_rect area, fx_flags flags) {
  fx_window *window = calloc(1, sizeof *window);
  strncpy(window->title, title, sizeof window->title - 1);
  window->area = area;
  window->flags = flags;
  fx_counter += 1;
  return window;
}

void fx_window_destroy(fx_window *window) { free(window); fx_counter -= 1; }
uint32_t fx_window_area(const fx_window *window) { return window->area.w * window->area.h; }
bool fx_rect_contains(const fx_rect *rect, fx_point point) {
  return point.x >= rect->x && point.y >= rect->y && point.x < rect->x + (int32_t)rect->w && point.y < rect->y + (int32_t)rect->h;
}
int fx_sum(int count, ...) {
  va_list args; va_start(args, count); int total = 0;
  for (int i = 0; i < count; i++) total += va_arg(args, int);
  va_end(args); return total;
}
size_t fx_copy_title(char *buffer, size_t size, const fx_window *window) {
  size_t length = strlen(window->title);
  if (size > 0) { strncpy(buffer, window->title, size - 1); buffer[size - 1] = 0; }
  return length;
}
void fx_events_fire(fx_events *events, int code) {
  events->stats.count += 1;
  if (events->filter == NULL || events->filter(NULL)) events->on_event(events->user, code);
}
fx_value fx_value_of_int(int64_t value) { fx_value result; result.integer = value; return result; }
int fx_bits_total(struct fx_bits *bits) { return bits->a + bits->b; }
int fx_match(int type) { return type * 10; }
long fx_log(fx_log_fn log, long level) { return log("from C", level); }
long double fx_precise(void) { return 1.0L; }
