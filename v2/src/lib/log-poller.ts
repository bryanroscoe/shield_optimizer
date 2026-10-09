/** Serial snapshot polling: bounded reads, no overlap, no late results after stop or navigation. */
export interface LogPollState<T> {
  busy: boolean;
  running: boolean;
  value: T | null;
  error: string | null;
}

export class LogPoller<T> {
  private state: LogPollState<T> = { busy: false, running: false, value: null, error: null };
  private generation = 0;
  private destroyed = false;
  private timer: ReturnType<typeof setTimeout> | null = null;

  constructor(
    private read: () => Promise<T>,
    private context: () => string | null,
    private changed: (state: LogPollState<T>) => void,
    private delayMs = 3000,
  ) {}

  private publish() {
    if (!this.destroyed) this.changed({ ...this.state });
  }

  start() {
    if (this.destroyed || this.state.busy || this.state.running || this.context() === null) return;
    this.state.running = true;
    void this.once();
  }

  stop() {
    this.state.running = false;
    ++this.generation;
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
    this.publish();
  }

  dispose() {
    this.destroyed = true;
    this.stop();
  }

  async once() {
    if (this.destroyed || this.state.busy) return;
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
    const context = this.context();
    if (context === null) { this.stop(); return; }
    const generation = ++this.generation;
    const current = () => !this.destroyed && generation === this.generation && this.context() === context;
    this.state.busy = true;
    this.state.error = null;
    this.state.value = null;
    this.publish();
    try {
      const value = await this.read();
      if (current()) this.state.value = value;
    } catch (error) {
      if (current()) {
        this.state.error = String(error);
        // A failed/unsupported read must not keep retrying the device forever.
        this.state.running = false;
      }
    } finally {
      this.state.busy = false;
      if (!current()) this.state.running = false;
      this.publish();
      if (current() && this.state.running) {
        this.timer = setTimeout(() => { this.timer = null; void this.once(); }, this.delayMs);
      }
    }
  }
}
