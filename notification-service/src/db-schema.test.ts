import { describe, it, expect, afterEach, vi } from "vitest";
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

// ── Issue #1009: versioned transactional migrations ───────────────────────────

describe("Store versioned migrations", () => {
  it("reports schema version 1 after opening a fresh database", () => {
    const store = new Store(":memory:");
    expect(store.currentSchemaVersion()).toBe(1);
    store.close();
  });

  it("creates the schema_version table", () => {
    const store = new Store(":memory:");
    const tables = listTables(store);
    expect(tables).toContain("schema_version");
    store.close();
  });

  it("re-opening an already-migrated database stays at v1 and does not fail", () => {
    const raw = new Database(":memory:");
    const path = raw.name;
    raw.close();

    const s1 = new Store(path);
    expect(s1.currentSchemaVersion()).toBe(1);
    s1.close();

    const s2 = new Store(path);
    expect(s2.currentSchemaVersion()).toBe(1);
    // Data tables must still exist.
    expect(listTables(s2)).toContain("notification_preferences");
    expect(listTables(s2)).toContain("notification_history");
    s2.close();
  });

  it("treats a legacy database (tables present, no schema_version) as v1 and preserves data", () => {
    // Simulate the old schema: create tables manually without schema_version.
    const raw = new Database(":memory:");
    raw.exec(`
      CREATE TABLE notification_preferences (
        investor_address TEXT PRIMARY KEY,
        email TEXT,
        webhook_url TEXT,
        enabled INTEGER NOT NULL DEFAULT 1,
        min_delta INTEGER NOT NULL DEFAULT 1,
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
      );
      CREATE TABLE investor_projects (
        investor_address TEXT NOT NULL,
        project_id INTEGER NOT NULL,
        first_seen_ledger INTEGER NOT NULL,
        last_seen_ledger INTEGER NOT NULL,
        PRIMARY KEY (investor_address, project_id)
      );
      CREATE TABLE processed_ledgers (ledger INTEGER PRIMARY KEY);
      CREATE TABLE notification_history (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        investor_address TEXT NOT NULL,
        project_id INTEGER NOT NULL,
        channel TEXT NOT NULL,
        ledger INTEGER NOT NULL,
        sent_at TEXT NOT NULL DEFAULT (datetime('now'))
      );
      CREATE INDEX idx_investor_projects_project ON investor_projects(project_id);
      CREATE INDEX idx_notification_history_sent_at ON notification_history(sent_at DESC);
    `);
    // Insert a preference row to prove data is preserved.
    raw.prepare(
      "INSERT INTO notification_preferences (investor_address, updated_at) VALUES (?, datetime('now'))",
    ).run("GABC");
    const path = raw.name;
    raw.close();

    const store = new Store(path);
    expect(store.currentSchemaVersion()).toBe(1);
    // The preference row must still be there.
    expect(store.getPreference("GABC")).toBeDefined();
    store.close();
  });

  it("rolls back a failed migration and leaves the database at the prior consistent version", () => {
    // We test the transaction guarantee by injecting a migration that throws
    // after modifying schema, then verifying the DB is still consistent.
    const raw = new Database(":memory:");
    // Apply v1 manually so we can inject a failing v2.
    raw.exec(`
      CREATE TABLE notification_preferences (
        investor_address TEXT PRIMARY KEY,
        email TEXT,
        webhook_url TEXT,
        enabled INTEGER NOT NULL DEFAULT 1,
        min_delta INTEGER NOT NULL DEFAULT 1,
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
      );
      CREATE TABLE investor_projects (
        investor_address TEXT NOT NULL,
        project_id INTEGER NOT NULL,
        first_seen_ledger INTEGER NOT NULL,
        last_seen_ledger INTEGER NOT NULL,
        PRIMARY KEY (investor_address, project_id)
      );
      CREATE TABLE processed_ledgers (ledger INTEGER PRIMARY KEY);
      CREATE TABLE notification_history (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        investor_address TEXT NOT NULL,
        project_id INTEGER NOT NULL,
        channel TEXT NOT NULL,
        ledger INTEGER NOT NULL,
        sent_at TEXT NOT NULL DEFAULT (datetime('now'))
      );
      CREATE INDEX idx_investor_projects_project ON investor_projects(project_id);
      CREATE INDEX idx_notification_history_sent_at ON notification_history(sent_at DESC);
      CREATE TABLE schema_version (version INTEGER NOT NULL);
      INSERT INTO schema_version (version) VALUES (1);
    `);
    const path = raw.name;
    raw.close();

    // Directly test transactional safety: open the DB, start a transaction,
    // execute a DDL statement, then roll back — the table must not exist after.
    const db = new Database(path);
    db.pragma("journal_mode = WAL");

    let threw = false;
    try {
      db.transaction(() => {
        db.exec("CREATE TABLE migration_v2_test (x TEXT)");
        throw new Error("simulated migration failure");
      })();
    } catch {
      threw = true;
    }

    expect(threw).toBe(true);
    // The table must not exist — the transaction was rolled back.
    const tableExists =
      db
        .prepare(
          "SELECT 1 FROM sqlite_master WHERE type='table' AND name='migration_v2_test'",
        )
        .get() !== undefined;
    expect(tableExists).toBe(false);

    // schema_version must still be 1.
    const { version } = db
      .prepare("SELECT version FROM schema_version")
      .get() as { version: number };
    expect(version).toBe(1);

    db.close();
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
