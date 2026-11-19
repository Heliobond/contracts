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

  it("creates the ledger_checkpoint table on a fresh database (not processed_ledgers)", () => {
    store = new Store(":memory:");
    const tables = listTables(store);
    expect(tables).toContain("ledger_checkpoint");
    expect(tables).not.toContain("processed_ledgers");
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

  it("does not move the cursor backward when an older ledger is marked", () => {
    store = new Store(":memory:");
    store.markLedgerProcessed(200);
    store.markLedgerProcessed(50);
    expect(store.getLastProcessedLedger()).toBe(200);
  });
});

// ── Issue #998: high-water mark migration ─────────────────────────────────────

describe("Store high-water mark migration (#998)", () => {
  it("migrates a multi-row processed_ledgers database preserving the max ledger", () => {
    // Build a legacy-style database with multiple rows in processed_ledgers.
    const raw = new Database(":memory:");
    raw.exec(`
      CREATE TABLE notification_preferences (
        investor_address TEXT PRIMARY KEY,
        email TEXT, webhook_url TEXT,
        enabled INTEGER NOT NULL DEFAULT 1,
        min_delta INTEGER NOT NULL DEFAULT 1,
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
      );
      CREATE TABLE investor_projects (
        investor_address TEXT NOT NULL, project_id INTEGER NOT NULL,
        first_seen_ledger INTEGER NOT NULL, last_seen_ledger INTEGER NOT NULL,
        PRIMARY KEY (investor_address, project_id)
      );
      CREATE TABLE processed_ledgers (ledger INTEGER PRIMARY KEY);
      CREATE TABLE notification_history (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        investor_address TEXT NOT NULL, project_id INTEGER NOT NULL,
        channel TEXT NOT NULL, ledger INTEGER NOT NULL,
        sent_at TEXT NOT NULL DEFAULT (datetime('now'))
      );
      CREATE INDEX idx_investor_projects_project ON investor_projects(project_id);
      CREATE INDEX idx_notification_history_sent_at ON notification_history(sent_at DESC);
    `);
    raw.prepare("INSERT INTO processed_ledgers VALUES (?)").run(100);
    raw.prepare("INSERT INTO processed_ledgers VALUES (?)").run(250);
    raw.prepare("INSERT INTO processed_ledgers VALUES (?)").run(175);
    const path = raw.name;
    raw.close();

    const store = new Store(path);
    // The highest ledger must be preserved.
    expect(store.getLastProcessedLedger()).toBe(250);
    // processed_ledgers must be gone.
    const tables = listTables(store);
    expect(tables).not.toContain("processed_ledgers");
    expect(tables).toContain("ledger_checkpoint");
    store.close();
  });

  it("migrates an empty processed_ledgers database to hwm = 0", () => {
    const raw = new Database(":memory:");
    raw.exec(`
      CREATE TABLE notification_preferences (
        investor_address TEXT PRIMARY KEY, email TEXT, webhook_url TEXT,
        enabled INTEGER NOT NULL DEFAULT 1, min_delta INTEGER NOT NULL DEFAULT 1,
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
      );
      CREATE TABLE investor_projects (
        investor_address TEXT NOT NULL, project_id INTEGER NOT NULL,
        first_seen_ledger INTEGER NOT NULL, last_seen_ledger INTEGER NOT NULL,
        PRIMARY KEY (investor_address, project_id)
      );
      CREATE TABLE processed_ledgers (ledger INTEGER PRIMARY KEY);
      CREATE TABLE notification_history (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        investor_address TEXT NOT NULL, project_id INTEGER NOT NULL,
        channel TEXT NOT NULL, ledger INTEGER NOT NULL,
        sent_at TEXT NOT NULL DEFAULT (datetime('now'))
      );
      CREATE INDEX idx_investor_projects_project ON investor_projects(project_id);
      CREATE INDEX idx_notification_history_sent_at ON notification_history(sent_at DESC);
    `);
    const path = raw.name;
    raw.close();

    const store = new Store(path);
    expect(store.getLastProcessedLedger()).toBe(0);
    store.close();
  });

  it("migration is idempotent — opening the migrated database again does not fail", () => {
    const raw = new Database(":memory:");
    raw.exec(`
      CREATE TABLE notification_preferences (
        investor_address TEXT PRIMARY KEY, email TEXT, webhook_url TEXT,
        enabled INTEGER NOT NULL DEFAULT 1, min_delta INTEGER NOT NULL DEFAULT 1,
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
      );
      CREATE TABLE investor_projects (
        investor_address TEXT NOT NULL, project_id INTEGER NOT NULL,
        first_seen_ledger INTEGER NOT NULL, last_seen_ledger INTEGER NOT NULL,
        PRIMARY KEY (investor_address, project_id)
      );
      CREATE TABLE processed_ledgers (ledger INTEGER PRIMARY KEY);
      CREATE TABLE notification_history (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        investor_address TEXT NOT NULL, project_id INTEGER NOT NULL,
        channel TEXT NOT NULL, ledger INTEGER NOT NULL,
        sent_at TEXT NOT NULL DEFAULT (datetime('now'))
      );
      CREATE INDEX idx_investor_projects_project ON investor_projects(project_id);
      CREATE INDEX idx_notification_history_sent_at ON notification_history(sent_at DESC);
    `);
    raw.prepare("INSERT INTO processed_ledgers VALUES (?)").run(300);
    const path = raw.name;
    raw.close();

    const s1 = new Store(path);
    s1.markLedgerProcessed(350);
    s1.close();

    // Second open: migration must not throw and cursor must survive.
    const s2 = new Store(path);
    expect(s2.getLastProcessedLedger()).toBe(350);
    s2.close();
  });
});
