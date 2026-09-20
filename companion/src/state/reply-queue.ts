// Turns the watch's stream of replies into something a command can await.
//
// The transport delivers lines whenever they arrive. A command wants the next
// reply that belongs to it, so replies land here and waiters take them in
// order.

export class ReplyQueue<T> {
  private ready: T[] = [];
  private waiting: { resolve: (value: T) => void; reject: (error: Error) => void; timer: unknown }[] =
    [];

  constructor(private readonly timeoutMs = 10000) {}

  // Called by the transport for every reply that arrives.
  push(reply: T) {
    const waiter = this.waiting.shift();
    if (waiter) {
      clearTimeout(waiter.timer as Parameters<typeof clearTimeout>[0]);
      waiter.resolve(reply);
      return;
    }
    this.ready.push(reply);
  }

  // Called by a command that wants the next reply.
  next(): Promise<T> {
    const ready = this.ready.shift();
    if (ready !== undefined) {
      return Promise.resolve(ready);
    }
    return new Promise<T>((resolve, reject) => {
      const entry = {
        resolve,
        reject,
        timer: setTimeout(() => {
          const index = this.waiting.indexOf(entry);
          if (index >= 0) {
            this.waiting.splice(index, 1);
          }
          reject(new Error('The watch did not reply.'));
        }, this.timeoutMs),
      };
      this.waiting.push(entry);
    });
  }

  // Drop anything left over, so a new command does not read a stale reply.
  reset() {
    this.ready = [];
    for (const waiter of this.waiting) {
      clearTimeout(waiter.timer as Parameters<typeof clearTimeout>[0]);
      waiter.reject(new Error('The watch disconnected.'));
    }
    this.waiting = [];
  }

  get pending(): number {
    return this.ready.length;
  }
}
