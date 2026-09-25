import assert from "node:assert/strict";
import test from "node:test";
import { createLivePoller } from "../utils/livePolling.ts";
const settle = () => new Promise<void>((resolve) => setImmediate(resolve));
function harness() {
  const requests: Array<{
    signal: AbortSignal;
    resolve: (value: number) => void;
    reject: (error: unknown) => void;
  }> = [];
  const timers = new Map<number, { run: () => void; delay: number }>();
  const data: number[] = [];
  const errors: unknown[] = [];
  let id = 0;
  let pending = false;
  const poller = createLivePoller({
    load: (signal) =>
      new Promise<number>((resolve, reject) =>
        requests.push({ signal, resolve, reject }),
      ),
    onData: (value) => data.push(value),
    onError: (error) => errors.push(error),
    onPending: (value) => {
      pending = value;
    },
    schedule: (run, delay) => {
      timers.set(++id, { run, delay });
      return id as unknown as ReturnType<typeof setTimeout>;
    },
    cancel: (timer) => {
      timers.delete(timer as unknown as number);
    },
  });
  return { poller, requests, timers, data, errors, pending: () => pending };
}
test("live polling never overlaps and waits ten seconds after a successful response", async () => {
  const h = harness();
  h.poller.start();
  h.poller.start();
  assert.equal(h.requests.length, 1);
  assert.equal(h.timers.size, 0);
  h.requests[0]!.resolve(7);
  await settle();
  assert.deepEqual(h.data, [7]);
  assert.equal(h.pending(), false);
  const [id, timer] = [...h.timers][0]!;
  assert.equal(timer.delay, 10000);
  h.timers.delete(id);
  timer.run();
  assert.equal(h.requests.length, 2);
  h.poller.stop();
  assert.equal(h.requests[1]!.signal.aborted, true);
});
test("pause, hide, or unmount cancels polling and ignores stale responses after restart", async () => {
  const h = harness();
  h.poller.start();
  h.poller.stop();
  assert.equal(h.requests[0]!.signal.aborted, true);
  h.poller.start();
  h.requests[0]!.resolve(99);
  await settle();
  assert.deepEqual(h.data, []);
  h.requests[1]!.resolve(2);
  await settle();
  assert.deepEqual(h.data, [2]);
  h.poller.stop();
  assert.equal(h.timers.size, 0);
});
test("failures retain prior data and back off; recovery resets the interval", async () => {
  const h = harness();
  h.poller.start();
  h.requests[0]!.resolve(4);
  await settle();
  for (const delay of [20000, 40000, 60000]) {
    const [id, timer] = [...h.timers][0]!;
    h.timers.delete(id);
    timer.run();
    h.requests.at(-1)!.reject(new Error("offline"));
    await settle();
    assert.equal([...h.timers.values()][0]!.delay, delay);
    assert.deepEqual(h.data, [4]);
  }
  const [id, timer] = [...h.timers][0]!;
  h.timers.delete(id);
  timer.run();
  h.requests.at(-1)!.resolve(5);
  await settle();
  assert.equal([...h.timers.values()][0]!.delay, 10000);
  assert.deepEqual(h.data, [4, 5]);
  h.poller.stop();
});
