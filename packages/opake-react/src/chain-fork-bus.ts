// In-React fan-out for `chain:forked` SSE events.
//
// The SDK exposes `opake.watchChainForks(handler)` as a single-subscriber
// surface. React mutations need per-instance subscriptions — every
// `useTreeMutation` call may want to know about forks affecting its
// scope. Rather than have each hook open its own SDK watcher (N WASM
// callbacks for N mutation hooks), `OpakeProvider` installs a single
// SDK-level watcher and forwards events to this bus, which fans out to
// every interested React hook.
//
// Empty subscribers list is the steady state for apps with no active
// mutations — the SDK watcher still runs, the bus iterates an empty
// list, no work done.

import type { ChainForkedEvent } from "@opake/sdk";

export type ChainForkHandler = (event: ChainForkedEvent) => void;

export class ChainForkBus {
  private readonly handlers = new Set<ChainForkHandler>();

  /** Register a callback. Returns an unsubscribe function. */
  subscribe(handler: ChainForkHandler): () => void {
    this.handlers.add(handler);
    return () => {
      this.handlers.delete(handler);
    };
  }

  /** Fan out an event to every subscriber. */
  dispatch(event: ChainForkedEvent): void {
    this.handlers.forEach((handler) => {
      try {
        handler(event);
      } catch (err) {
        // One handler throwing must not break sibling handlers.
        console.warn("[opake-react] chain-fork handler threw:", err);
      }
    });
  }

  /** Test helper: number of active subscribers. */
  subscriberCount(): number {
    return this.handlers.size;
  }
}
