# spectral-norm: the largest eigenvalue of an infinite matrix by the power method, as the Computer
# Language Benchmarks Game writes it. Numeric loops over lists. The argument is the size.
import math
import sys


def a(i, j):
    return 1.0 / ((i + j) * (i + j + 1) // 2 + i + 1)


def mul_av(v, out):
    n = len(v)
    for i in range(n):
        total = 0.0
        for j in range(n):
            total += a(i, j) * v[j]
        out[i] = total


def mul_atv(v, out):
    n = len(v)
    for i in range(n):
        total = 0.0
        for j in range(n):
            total += a(j, i) * v[j]
        out[i] = total


def mul_atav(v, out, tmp):
    mul_av(v, tmp)
    mul_atv(tmp, out)


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 100
    u = [1.0] * n
    v = [0.0] * n
    tmp = [0.0] * n

    for _ in range(10):
        mul_atav(u, v, tmp)
        mul_atav(v, u, tmp)

    vbv = 0.0
    vv = 0.0
    for i in range(n):
        vbv += u[i] * v[i]
        vv += v[i] * v[i]

    print(f"{math.sqrt(vbv / vv):.9f}")


main()
