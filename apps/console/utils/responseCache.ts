/** Session-memory snapshots only: no localStorage, cookies or shared SSR state. */
export class ResponseCache {
  private entries = new Map<string, { json: string; at: number }>();
  private scope = "";
  private bytes = 0;
  generation = 0;
  private now: () => number;
  constructor(now: () => number = Date.now) {
    this.now = now;
  }

  setScope(scope: string) {
    if (scope === this.scope) return;
    this.scope = scope;
    this.clear();
  }
  clear() {
    this.entries.clear();
    this.bytes = 0;
    this.generation++;
  }
  read<T>(key: string): { value: T; fresh: boolean } | undefined {
    const entry = this.entries.get(key);
    if (!entry) return;
    if (this.now() - entry.at > 300_000) {
      this.remove(key);
      return;
    }
    this.entries.delete(key);
    this.entries.set(key, entry);
    return {
      value: JSON.parse(entry.json) as T,
      fresh: this.now() - entry.at < 30_000,
    };
  }
  write(key: string, value: unknown, generation = this.generation) {
    // A response started before logout, permission changes or writes cannot refill the cache.
    if (generation !== this.generation) return;
    const json = JSON.stringify(value);
    if (!json || (json.length + key.length) * 2 > 300_000) return;
    this.remove(key);
    this.entries.set(key, { json, at: this.now() });
    this.bytes += (json.length + key.length) * 2;
    while (this.entries.size > 32 || this.bytes > 2_000_000) {
      const oldest = this.entries.keys().next().value;
      if (oldest === undefined) break;
      this.remove(oldest);
    }
  }
  private remove(key: string) {
    const entry = this.entries.get(key);
    if (entry) this.bytes -= (entry.json.length + key.length) * 2;
    this.entries.delete(key);
  }
}
