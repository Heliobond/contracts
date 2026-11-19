import { describe, it, expect, afterEach } from "vitest";
import Database from "better-sqlite3";
import { Store, SourceIdentityMismatchError } from "./db";

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

// ── Issue #1012: source identity binding ──────────────────────────────────────

const IDENTITY_A = {
  networkPassphrase: "Test SDF Network ; September 2015",
  registryContractId: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4",
};

const IDENTITY_B = {
  networkPassphrase: "Public Global Stellar Network ; September 2015",
  registryContractId: "CBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBSC4",
};

describe("Store source identity", () => {
  it("writes identity on a fresh database", () => {
    const store = new Store(":memory:", IDENTITY_A);
    const db = (store as unknown as { db: Database.Database }).db;
    const rows = db
      .prepare("SELECT key, value FROM source_identity ORDER BY key")
      .all() as { key: string; value: string }[];
    expect(rows).toContainEqual({ key: "network_passphrase", value: IDENTITY_A.networkPassphrase });
    expect(rows).toContainEqual({ key: "registry_contract_id", value: IDENTITY_A.registryContractId });
    store.close();
  });

  it("accepts the same identity on an existing database", () => {
    const db = new Database(":memory:");
    const path = db.name;
    db.close();

    const s1 = new Store(path, IDENTITY_A);
    s1.close();
    expect(() => new Store(path, IDENTITY_A).close()).not.toThrow();
  });

  it("throws SourceIdentityMismatchError when network_passphrase differs", () => {
    const db = new Database(":memory:");
    const path = db.name;
    db.close();

    const s1 = new Store(path, IDENTITY_A);
    s1.close();

    expect(() =>
      new Store(path, { ...IDENTITY_A, networkPassphrase: IDENTITY_B.networkPassphrase }),
    ).toThrow(SourceIdentityMismatchError);
  });

  it("throws SourceIdentityMismatchError when registry_contract_id differs", () => {
    const db = new Database(":memory:");
    const path = db.name;
    db.close();

    const s1 = new Store(path, IDENTITY_A);
    s1.close();

    expect(() =>
      new Store(path, { ...IDENTITY_A, registryContractId: IDENTITY_B.registryContractId }),
    ).toThrow(SourceIdentityMismatchError);
  });

  it("includes the stored and provided values in the error message", () => {
    const db = new Database(":memory:");
    const path = db.name;
    db.close();

    const s1 = new Store(path, IDENTITY_A);
    s1.close();

    let err: unknown;
    try {
      new Store(path, IDENTITY_B);
    } catch (e) {
      err = e;
    }
    expect(err).toBeInstanceOf(SourceIdentityMismatchError);
    const msg = (err as SourceIdentityMismatchError).message;
    expect(msg).toContain(IDENTITY_A.networkPassphrase);
    expect(msg).toContain(IDENTITY_B.networkPassphrase);
  });

  it("does not check identity when no identity argument is passed", () => {
    expect(() => new Store(":memory:").close()).not.toThrow();
  });
});

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
