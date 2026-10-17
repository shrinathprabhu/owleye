import assert from "node:assert/strict";
import test from "node:test";
import {
  ReportWarmupQueue,
  reportRequestKey,
  reportWarmups,
  waitForReport,
} from "../utils/reportWarmup.ts";

const tick = () => new Promise((resolve) => setTimeout(resolve, 10));

test("background warming waits for visible reads, deduplicates, and never exceeds two requests", async () => {
  const queue = new ReportWarmupQueue(0);
  const started: string[] = [];
  const finish: Array<() => void> = [];
  queue.beginForeground();
  const enqueue = (key: string) =>
    queue.enqueue({
      key,
      run: () =>
        new Promise<void>((resolve) => {
          started.push(key);
          finish.push(resolve);
        }),
    });
  for (const key of ["a", "a", "b", "c", "d", "ignored-over-bound"])
    enqueue(key);
  await tick();
  assert.equal(started.length, 0);
  queue.endForeground();
  await tick();
  assert.deepEqual(started, ["a", "b"]);
  queue.beginForeground();
  finish[0]!();
  await tick();
  assert.equal(started.length, 2);
  queue.endForeground();
  await tick();
  assert.deepEqual(started, ["a", "b", "c"]);
  finish[1]!();
  finish[2]!();
  await tick();
  assert.deepEqual(started, ["a", "b", "c", "d"]);
  finish[3]!();
  await tick();
  enqueue("a");
  await tick();
  assert.equal(started.length, 4);
  queue.clear();
});

test("scope invalidation aborts active work, discards old queued work and does not free slots early", async () => {
  const queue = new ReportWarmupQueue(0);
  const signals: AbortSignal[] = [];
  const finish: Array<() => void> = [];
  let nextStarted = false;
  for (const key of ["old-a", "old-b", "never"])
    queue.enqueue({
      key,
      run: (signal) =>
        new Promise<void>((resolve) => {
          signals.push(signal);
          finish.push(resolve);
        }),
    });
  await tick();
  queue.clear();
  assert.ok(signals.every((signal) => signal.aborted));
  queue.enqueue({
    key: "new-site",
    run: async () => {
      nextStarted = true;
    },
  });
  await tick();
  assert.equal(nextStarted, false);
  finish[0]!();
  finish[1]!();
  await tick();
  await tick();
  assert.equal(nextStarted, true);
  assert.equal(signals.length, 2);
  queue.clear();
});

test("warmup allowlist excludes the visible report and all AI, credentials, settings and live endpoints", () => {
  const reports = reportWarmups("owl_site", "/events");
  assert.equal(reports.length, 3);
  assert.ok(
    reports.every(
      (report) =>
        !/\/(?:events|prompt|ai|live|settings|keys|billing|history)(?:\/|$)/.test(
          report.url,
        ),
    ),
  );
  assert.equal(reportWarmups("", "/").length, 0);
  assert.ok(reports.every((report) => report.key.includes("owl_site")));
  assert.equal(
    reportRequestKey("/a", { y: undefined, b: 2, a: 1 }),
    reportRequestKey("/a", { a: 1, b: 2 }),
  );
});

test("canceling a visible waiter cannot cancel another consumer of the warmed read", async () => {
  let finish!: (value: number) => void;
  const shared = new Promise<number>((resolve) => {
    finish = resolve;
  });
  const controller = new AbortController();
  const first = waitForReport(shared, controller.signal);
  const second = waitForReport(shared);
  controller.abort();
  await assert.rejects(first, { name: "AbortError" });
  finish(7);
  assert.equal(await second, 7);
});

test("long-lived annotation refreshes keep attempted-key bookkeeping bounded", async () => {
  const queue = new ReportWarmupQueue(0);
  for (let index = 0; index < 68; index++) {
    assert.equal(
      queue.enqueue({ key: String(index), run: async () => {} }),
      true,
    );
    await tick();
  }
  assert.equal(queue.enqueue({ key: "67", run: async () => {} }), false);
  assert.equal(queue.enqueue({ key: "0", run: async () => {} }), true);
  queue.clear();
});
