// spectral-norm: the largest eigenvalue of an infinite matrix by the power method, as the Computer
// Language Benchmarks Game writes it. Numeric loops over arrays. The argument is the size.
"use strict";

function a(i, j) {
  return 1.0 / (((i + j) * (i + j + 1)) / 2 + i + 1);
}

function mulAv(v, out) {
  const n = v.length;
  for (let i = 0; i < n; i++) {
    let sum = 0.0;
    for (let j = 0; j < n; j++) {
      sum += a(i, j) * v[j];
    }
    out[i] = sum;
  }
}

function mulAtv(v, out) {
  const n = v.length;
  for (let i = 0; i < n; i++) {
    let sum = 0.0;
    for (let j = 0; j < n; j++) {
      sum += a(j, i) * v[j];
    }
    out[i] = sum;
  }
}

function mulAtAv(v, out, tmp) {
  mulAv(v, tmp);
  mulAtv(tmp, out);
}

function main() {
  const n = process.argv.length > 2 ? Number(process.argv[2]) : 100;
  const u = new Array(n).fill(1.0);
  const v = new Array(n).fill(0.0);
  const tmp = new Array(n).fill(0.0);

  for (let step = 0; step < 10; step++) {
    mulAtAv(u, v, tmp);
    mulAtAv(v, u, tmp);
  }

  let vbv = 0.0;
  let vv = 0.0;
  for (let i = 0; i < n; i++) {
    vbv += u[i] * v[i];
    vv += v[i] * v[i];
  }

  console.log(Math.sqrt(vbv / vv).toFixed(9));
}

main();
