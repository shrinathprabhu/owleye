/** One request at a time. Cancelling invalidates even a response that ignores abort. */
export function createLivePoller<T>(options: {
  load: (signal: AbortSignal) => Promise<T>;
  onData: (value: T) => void;
  onError: (error: unknown) => void;
  onPending: (pending: boolean) => void;
  schedule?: (
    callback: () => void,
    delay: number,
  ) => ReturnType<typeof setTimeout>;
  cancel?: (timer: ReturnType<typeof setTimeout>) => void;
}) {
  const schedule = options.schedule ?? setTimeout;
  const cancel = options.cancel ?? clearTimeout;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let controller: AbortController | undefined;
  let running = false;
  let generation = 0;
  let failures = 0;
  async function tick() {
    if (!running || controller) return;
    const version = generation;
    const current = new AbortController();
    controller = current;
    options.onPending(true);
    try {
      const data = await options.load(current.signal);
      if (version !== generation) return;
      failures = 0;
      options.onData(data);
    } catch (error) {
      if (version !== generation) return;
      failures += 1;
      options.onError(error);
    } finally {
      if (version === generation) {
        controller = undefined;
        options.onPending(false);
        if (running)
          timer = schedule(
            () => {
              timer = undefined;
              void tick();
            },
            Math.min(60_000, 10_000 * 2 ** Math.min(failures, 3)),
          );
      }
    }
  }
  function stop() {
    running = false;
    generation += 1;
    if (timer !== undefined) cancel(timer);
    timer = undefined;
    controller?.abort();
    controller = undefined;
    options.onPending(false);
  }
  return {
    start() {
      if (!running) {
        running = true;
        failures = 0;
        void tick();
      }
    },
    stop,
    refresh() {
      stop();
      running = true;
      failures = 0;
      void tick();
    },
  };
}
