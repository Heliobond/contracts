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
import { describe, it, expect, afterEach } from "vitest";
import Database from "better-sqlite3";
import { Store } from "./db";

describe("Store schema initialization", () => {
  let store: Store;

  afterEach(() => {
    store?.close();
  });

  it("creates the notification_preferences table on a fresh database", () => {
    store = new Store(":memory:");
    const tables = listTables(store);
    expect(tables).toContain("notification_preferences");
  });

  it("creates the investor_projects table on a fresh database", () => {
    store = new Store(":memory:");
    const tables = listTables(store);
    expect(tables).toContain("investor_projects");
  });

  it("creates the processed_ledgers table on a fresh database", () => {
    store = new Store(":memory:");
    const tables = listTables(store);
    expect(tables).toContain("processed_ledgers");
  });

  it("creates the notification_history table on a fresh database", () => {
    store = new Store(":memory:");
    const tables = listTables(store);
    expect(tables).toContain("notification_history");
  });

  it("creates indexes on investor_projects and notification_history", () => {
    store = new Store(":memory:");
    const indexes = listIndexes(store);
    expect(indexes).toContain("idx_investor_projects_project");
    expect(indexes).toContain("idx_notification_history_sent_at");
  });

  it("applies migrations idempotently — opening the same DB twice does not fail", () => {
    const db = new Database(":memory:");
    const path = db.name;
    db.close();

    const store1 = new Store(path);
    store1.close();

    // Re-opening should succeed without errors (CREATE TABLE IF NOT EXISTS).
    store = new Store(path);
    const tables = listTables(store);
    expect(tables).toContain("notification_preferences");
    expect(tables).toContain("notification_history");
  });
});

/** Return the set of user-created table names in the database. */
function listTables(store: Store): string[] {
  const db = (store as unknown as { db: Database.Database }).db;
  const rows = db
    .prepare(
      "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
    .all() as { name: string }[];
  return rows.map((r) => r.name);
}

/** Return the set of user-created index names in the database. */
function listIndexes(store: Store): string[] {
  const db = (store as unknown as { db: Database.Database }).db;
  const rows = db
    .prepare(
      "SELECT name FROM sqlite_master WHERE type = 'index' AND name NOT LIKE 'sqlite_%'",
    )
    .all() as { name: string }[];
  return rows.map((r) => r.name);
}

// ── Issue #393: markLedgerProcessed / getLastProcessedLedger ─────────────────

describe("Store processed-ledger cursor", () => {
  let store: Store;

  afterEach(() => {
    store?.close();
  });

  it("returns 0 on a fresh database", () => {
    store = new Store(":memory:");
    expect(store.getLastProcessedLedger()).toBe(0);
  });

  it("returns the inserted ledger after a single mark", () => {
    store = new Store(":memory:");
    store.markLedgerProcessed(42);
    expect(store.getLastProcessedLedger()).toBe(42);
  });

  it("returns the maximum ledger across out-of-order inserts", () => {
    store = new Store(":memory:");
    store.markLedgerProcessed(100);
    store.markLedgerProcessed(50);
    store.markLedgerProcessed(200);
    store.markLedgerProcessed(75);
    expect(store.getLastProcessedLedger()).toBe(200);
  });

  it("ignores duplicate ledger numbers", () => {
    store = new Store(":memory:");
    store.markLedgerProcessed(100);
    store.markLedgerProcessed(100);
    store.markLedgerProcessed(100);
    expect(store.getLastProcessedLedger()).toBe(100);
  });
});
