// channels: a producer worker thread sends `0..n` as messages and the main thread receives and adds
// them. The messages of `worker_threads`. The argument is `n`.
"use strict";

const { Worker, isMainThread, parentPort, workerData } = require("worker_threads");

if (isMainThread) {
  const n = process.argv.length > 2 ? Number(process.argv[2]) : 1000;
  const producer = new Worker(__filename, { workerData: { n } });
  let count = 0;
  let sum = 0;

  producer.on("message", (value) => {
    if (value === null) {
      console.log(`received ${count} sum ${sum}`);
      return;
    }
    count += 1;
    sum += value;
  });
} else {
  for (let i = 0; i < workerData.n; i++) {
    parentPort.postMessage(i);
  }
  parentPort.postMessage(null);
}
