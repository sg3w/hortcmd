// ============================================================
// Shared test environment.
//
// jsdom in this Node build exposes no working `localStorage`, and the
// settings/session stores persist through it. Without a shim, importing
// any store from a test throws before the first assertion runs — so one
// in-memory implementation stands in, fresh per test file.
// ============================================================

class MemoryStorage implements Storage {
  private data = new Map<string, string>();

  get length(): number {
    return this.data.size;
  }
  clear(): void {
    this.data.clear();
  }
  getItem(key: string): string | null {
    return this.data.get(key) ?? null;
  }
  key(index: number): string | null {
    return [...this.data.keys()][index] ?? null;
  }
  removeItem(key: string): void {
    this.data.delete(key);
  }
  setItem(key: string, value: string): void {
    this.data.set(key, String(value));
  }
}

for (const target of [globalThis, globalThis.window]) {
  if (!target) continue;
  const host = target as unknown as { localStorage?: Storage };
  if (!host.localStorage) {
    Object.defineProperty(host, "localStorage", {
      value: new MemoryStorage(),
      configurable: true,
    });
  }
}
