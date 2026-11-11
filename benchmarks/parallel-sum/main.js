// parallel-sum: four worker threads add `i * i % 1000` over their quarter of `0..n`, and the main
// thread adds their results. Threads and joins, with `worker_threads`. The argument is `n`.
"use strict";

const { Worker, isMainThread, parentPort, workerData } = require("worker_threads");

const THREADS = 4;

function partial(start, end) {
  let sum = 0;
  for (let i = start; i < end; i++) {
    sum += (i * i) % 1000;
  }
  return sum;
}

if (isMainThread) {
  const n = process.argv.length > 2 ? Number(process.argv[2]) : 1000;
  const chunk = Math.floor(n / THREADS);
  const results = [];

  for (let index = 0; index < THREADS; index++) {
    const start = index * chunk;
    const end = index === THREADS - 1 ? n : start + chunk;

    results.push(
      new Promise((resolve, reject) => {
        const worker = new Worker(__filename, { workerData: { start, end } });
        worker.on("message", resolve);
        worker.on("error", reject);
      }),
    );
  }

  Promise.all(results).then((sums) => console.log(`${sums.reduce((a, b) => a + b, 0)}`));
} else {
  parentPort.postMessage(partial(workerData.start, workerData.end));
}
