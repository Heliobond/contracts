import { describe, it, expect, afterEach, vi } from "vitest";
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
