import { describe, it, expect, vi } from "vitest";
import type { Server } from "http";
import { createShutdownHandler } from "./index";
import { PollHandle } from "./listener";
import { Store } from "./db";

function makePollHandle(): PollHandle & { stop: ReturnType<typeof vi.fn> } {
  return { stop: vi.fn(async () => undefined) };
}

function makeHttpServer(): Server {
  return {
    close: vi.fn((cb?: (err?: Error) => void) => cb?.()),
  } as unknown as Server;
}

function makeStore(): Store & { close: ReturnType<typeof vi.fn> } {
  return { close: vi.fn() } as unknown as Store & {
    close: ReturnType<typeof vi.fn>;
  };
}

describe("createShutdownHandler", () => {
  it("stops polling, closes the HTTP server, then closes the store", async () => {
    const pollHandle = makePollHandle();
    const httpServer = makeHttpServer();
    const store = makeStore();
    const order: string[] = [];

    pollHandle.stop.mockImplementation(async () => {
      order.push("poll-stopped");
    });
    (httpServer.close as ReturnType<typeof vi.fn>).mockImplementation(
      (cb?: (err?: Error) => void) => {
        order.push("http-closed");
        cb?.();
      },
    );
    store.close.mockImplementation(() => {
      order.push("store-closed");
    });

    const shutdown = createShutdownHandler(pollHandle, httpServer, store);
    await shutdown("SIGTERM");

    expect(order).toEqual(["poll-stopped", "http-closed", "store-closed"]);
  });

  it("waits for in-flight poll/event processing before closing connections", async () => {
    const pollHandle = makePollHandle();
    const httpServer = makeHttpServer();
    const store = makeStore();

    let inFlightResolved = false;
    pollHandle.stop.mockImplementation(
      () =>
        new Promise<void>((resolve) =>
          setTimeout(() => {
            inFlightResolved = true;
            resolve();
          }, 10),
        ),
    );

    const shutdown = createShutdownHandler(pollHandle, httpServer, store);
    await shutdown("SIGINT");

    expect(inFlightResolved).toBe(true);
    expect(httpServer.close).toHaveBeenCalledTimes(1);
    expect(store.close).toHaveBeenCalledTimes(1);
  });

  it("is idempotent — a second signal does not repeat shutdown work", async () => {
    const pollHandle = makePollHandle();
    const httpServer = makeHttpServer();
    const store = makeStore();

    const shutdown = createShutdownHandler(pollHandle, httpServer, store);
    await shutdown("SIGTERM");
    await shutdown("SIGTERM");

    expect(pollHandle.stop).toHaveBeenCalledTimes(1);
    expect(httpServer.close).toHaveBeenCalledTimes(1);
    expect(store.close).toHaveBeenCalledTimes(1);
  });
});
