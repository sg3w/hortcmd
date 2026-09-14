import type { FileIconRequest } from "@/ipc/bindings/FileIconRequest";
import type { FileIconResult } from "@/ipc/bindings/FileIconResult";

type Listener = (image: string | null) => void;
type Job = { request: FileIconRequest; listeners: Set<Listener> };
/** One batch in flight, 16 icons/batch, 512 waiting identities, 256-entry/4 MiB LRU.
 * No object URLs, disk persistence, or filesystem work. Unmounted jobs are removed.
 */
export class IconQueue {
  private cache = new Map<string, { image: string | null; expires: number; bytes: number }>();
  private jobs = new Map<string, Job>();
  private waiting = new Set<string>();
  private bytes = 0;
  private running = false;
  private timer: ReturnType<typeof setTimeout> | undefined;
  constructor(private fetch: (r: FileIconRequest[]) => Promise<FileIconResult[]>,
    private now = Date.now, private maxEntries = 256, private maxBytes = 4 * 1024 * 1024) {}

  subscribe(request: FileIconRequest, listener: Listener): () => void {
    const key = request.id;
    const cached = this.cache.get(key);
    if (cached && cached.expires > this.now()) {
      this.cache.delete(key); this.cache.set(key, cached);
      listener(cached.image);
      return () => {};
    }
    let job = this.jobs.get(key);
    if (!job) {
      if (this.waiting.size >= 512) return () => {};
      job = { request, listeners: new Set() };
      this.jobs.set(key, job); this.waiting.add(key);
    }
    job.listeners.add(listener);
    this.schedule();
    return () => {
      job.listeners.delete(listener);
      if (!job.listeners.size && this.waiting.delete(key)) this.jobs.delete(key);
    };
  }

  private schedule() {
    if (!this.running && !this.timer && this.waiting.size) {
      this.timer = setTimeout(() => { this.timer = undefined; void this.flush(); }, 12);
    }
  }
  private async flush() {
    const batch = [...this.waiting].slice(0, 16).map(k => this.jobs.get(k)!);
    batch.forEach(j => this.waiting.delete(j.request.id));
    if (!batch.length) return;
    this.running = true;
    let results: FileIconResult[] = [];
    try { results = await this.fetch(batch.map(j => j.request)); } catch { /* quiet fallback */ }
    const images = new Map(results.map(r => [r.id, r.image]));
    for (const job of batch) {
      const key = job.request.id;
      const raw = images.get(key);
      const image = raw?.startsWith("data:image/png;base64,") && raw.length <= 132000 ? raw : null;
      const old = this.cache.get(key);
      this.bytes -= old?.bytes ?? 0;
      this.cache.delete(key);
      const bytes = key.length * 2 + (image?.length ?? 0) * 2;
      this.cache.set(key, { image, bytes, expires: this.now() + (image ? 60000 : 5000) });
      this.bytes += bytes;
      while (this.cache.size > this.maxEntries || this.bytes > this.maxBytes) {
        const oldest = this.cache.keys().next().value!;
        this.bytes -= this.cache.get(oldest)!.bytes;
        this.cache.delete(oldest);
      }
      this.jobs.delete(key);
      job.listeners.forEach(listener => listener(image));
    }
    this.running = false;
    this.schedule();
  }
  /** Read-only counters for repeatable queue/memory tests. */
  stats() { return { entries: this.cache.size, bytes: this.bytes, waiting: this.waiting.size, running: this.running }; }
}
