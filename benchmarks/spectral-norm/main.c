/* spectral-norm: the largest eigenvalue of an infinite matrix by the power method, as the Computer
 * Language Benchmarks Game writes it. Numeric loops over arrays. The argument is the size. */
#include <math.h>
#include <stdio.h>
#include <stdlib.h>

static double a(size_t i, size_t j) {
  return 1.0 / (double)((i + j) * (i + j + 1) / 2 + i + 1);
}

static void mul_av(const double *v, double *out, size_t n) {
  for (size_t i = 0; i < n; i++) {
    double sum = 0.0;
    for (size_t j = 0; j < n; j++) {
      sum += a(i, j) * v[j];
    }
    out[i] = sum;
  }
}

static void mul_atv(const double *v, double *out, size_t n) {
  for (size_t i = 0; i < n; i++) {
    double sum = 0.0;
    for (size_t j = 0; j < n; j++) {
      sum += a(j, i) * v[j];
    }
    out[i] = sum;
  }
}

static void mul_atav(const double *v, double *out, double *tmp, size_t n) {
  mul_av(v, tmp, n);
  mul_atv(tmp, out, n);
}

int main(int argc, char **argv) {
  size_t n = argc > 1 ? (size_t)strtoull(argv[1], NULL, 10) : 100;
  double *u = malloc(n * sizeof(double));
  double *v = malloc(n * sizeof(double));
  double *tmp = malloc(n * sizeof(double));
  if (u == NULL || v == NULL || tmp == NULL) {
    return 1;
  }
  for (size_t i = 0; i < n; i++) {
    u[i] = 1.0;
    v[i] = 0.0;
  }
  for (int step = 0; step < 10; step++) {
    mul_atav(u, v, tmp, n);
    mul_atav(v, u, tmp, n);
  }
  double vbv = 0.0, vv = 0.0;
  for (size_t i = 0; i < n; i++) {
    vbv += u[i] * v[i];
    vv += v[i] * v[i];
  }
  printf("%.9f\n", sqrt(vbv / vv));
  free(u);
  free(v);
  free(tmp);
  return 0;
}
