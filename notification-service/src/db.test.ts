import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const { failTarget, instances } = vi.hoisted(() => ({
  failTarget: { value: 0 },
  instances: [] as Array<{
    pragma: ReturnType<typeof vi.fn>;
    exec: ReturnType<typeof vi.fn>;
    prepare: ReturnType<typeof vi.fn>;
    close: ReturnType<typeof vi.fn>;
  }>,
}));

let callCount = 0;

vi.mock("better-sqlite3", () => ({
  default: vi.fn().mockImplementation(() => {
    if (callCount < failTarget.value) {
      callCount++;
      throw new Error("SQLITE_CANTOPEN: unable to open database file");
    }
    const instance = {
      pragma: vi.fn(),
      exec: vi.fn(),
      prepare: vi.fn().mockReturnValue({
        get: vi.fn(),
        all: vi.fn(),
        run: vi.fn(),
      }),
      close: vi.fn(),
    };
    instances.push(instance);
    return instance;
  }),
}));

import { Store } from "./db";

describe("Store connection retry", () => {
  const origSleep = Store.sleep;

  beforeEach(() => {
    callCount = 0;
    failTarget.value = 0;
    instances.length = 0;
    // Skip real delays in tests.
    Store.sleep = vi.fn();
  });

  afterEach(() => {
    Store.sleep = origSleep;
  });

  it("creates a working store on the first attempt when the DB is ready", () => {
    failTarget.value = 0;
    const store = new Store(":memory:");

    expect(instances).toHaveLength(1);
    store.close();
  });

  it("retries and eventually connects when the DB becomes ready", () => {
    failTarget.value = 2;
    const store = new Store(":memory:");

    // 2 failures + 1 success = 3 total attempts
    expect(instances).toHaveLength(1);
    expect(callCount).toBe(2);
    store.close();
  });

  it("throws after exhausting all retry attempts", () => {
    failTarget.value = 10; // more than max retries + 1

    expect(() => new Store(":memory:")).toThrow(/SQLITE_CANTOPEN/);
    expect(instances).toHaveLength(0);
  });
});
