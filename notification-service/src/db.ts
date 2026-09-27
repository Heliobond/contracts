import Database from "better-sqlite3";
import {
  NotificationHistoryEntry,
  NotificationHistoryPage,
  NotificationPreference,
} from "./types";

/** Max retries when the database isn't reachable on startup. */
const DB_CONNECT_MAX_RETRIES = 5;
/** Base delay (ms) between retry attempts — doubles each attempt. */
const DB_CONNECT_RETRY_BASE_DELAY_MS = 200;

/**
 * Synchronous sleep used by the retry loop. Extracted so tests can override
 * via `Store.sleep = …` to avoid real delays.
 */
function defaultSleep(ms: number): void {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    /* spin */
  }
}

export class Store {
  /** Overridable sleep function for testing. */
  static sleep: (ms: number) => void = defaultSleep;

  private db: Database.Database;

  constructor(path: string) {
    this.db = Store.connectWithRetry(path);
    this.db.pragma("journal_mode = WAL");
    this.migrate();
  }

  /**
   * Open a SQLite database, retrying with exponential backoff if the file
   * isn't available yet (e.g. a mounted volume that arrives after the
   * container starts). Throws after exhausting all attempts.
   */
  static connectWithRetry(path: string): Database.Database {
    let lastError: unknown;
    for (let attempt = 0; attempt <= DB_CONNECT_MAX_RETRIES; attempt++) {
      try {
        return new Database(path);
      } catch (err) {
        lastError = err;
        if (attempt < DB_CONNECT_MAX_RETRIES) {
          const delayMs = DB_CONNECT_RETRY_BASE_DELAY_MS * 2 ** attempt;
          console.warn(
            `[db] Connection attempt ${attempt + 1}/${DB_CONNECT_MAX_RETRIES + 1} failed, retrying in ${delayMs}ms…`,
          );
          Store.sleep(delayMs);
        }
      }
    }
    throw lastError;
  }

  private migrate(): void {
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS notification_preferences (
        investor_address TEXT PRIMARY KEY,
        email TEXT,
        webhook_url TEXT,
        enabled INTEGER NOT NULL DEFAULT 1,
        min_delta INTEGER NOT NULL DEFAULT 1,
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
      );

      CREATE TABLE IF NOT EXISTS investor_projects (
        investor_address TEXT NOT NULL,
        project_id INTEGER NOT NULL,
        first_seen_ledger INTEGER NOT NULL,
        last_seen_ledger INTEGER NOT NULL,
        PRIMARY KEY (investor_address, project_id)
      );

      CREATE TABLE IF NOT EXISTS processed_ledgers (
        ledger INTEGER PRIMARY KEY
      );

      CREATE TABLE IF NOT EXISTS notification_history (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        investor_address TEXT NOT NULL,
        project_id INTEGER NOT NULL,
        channel TEXT NOT NULL,
        ledger INTEGER NOT NULL,
        sent_at TEXT NOT NULL DEFAULT (datetime('now'))
      );

      CREATE INDEX IF NOT EXISTS idx_investor_projects_project
        ON investor_projects(project_id);

      CREATE INDEX IF NOT EXISTS idx_notification_history_sent_at
        ON notification_history(sent_at DESC);
    `);
  }

  // ── Notification preferences ───────────────────────────────────────────

  upsertPreference(pref: NotificationPreference): void {
    const stmt = this.db.prepare(`
      INSERT INTO notification_preferences
        (investor_address, email, webhook_url, enabled, min_delta, updated_at)
      VALUES (@investor_address, @email, @webhook_url, @enabled, @min_delta, @updated_at)
      ON CONFLICT(investor_address) DO UPDATE SET
        email = excluded.email,
        webhook_url = excluded.webhook_url,
        enabled = excluded.enabled,
        min_delta = excluded.min_delta,
        updated_at = excluded.updated_at
    `);
    stmt.run({ ...pref, enabled: pref.enabled ? 1 : 0 });
  }

  getPreference(address: string): NotificationPreference | undefined {
    const row = this.db
      .prepare(
        "SELECT * FROM notification_preferences WHERE investor_address = ?",
      )
      .get(address) as Record<string, unknown> | undefined;
    if (!row) return undefined;
    return this.rowToPreference(row);
  }

  listPreferences(): NotificationPreference[] {
    const rows = this.db
      .prepare(
        "SELECT * FROM notification_preferences ORDER BY updated_at DESC",
      )
      .all() as Record<string, unknown>[];
    return rows.map((r) => this.rowToPreference(r));
  }

  deletePreference(address: string): void {
    this.db
      .prepare(
        "DELETE FROM notification_preferences WHERE investor_address = ?",
      )
      .run(address);
  }

  // ── Investor-project index ─────────────────────────────────────────────

  recordInvestment(
    investor_address: string,
    project_id: number,
    ledger: number,
  ): void {
    this.db
      .prepare(
        `INSERT INTO investor_projects
           (investor_address, project_id, first_seen_ledger, last_seen_ledger)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(investor_address, project_id) DO UPDATE SET
           last_seen_ledger = excluded.last_seen_ledger`,
      )
      .run(investor_address, project_id, ledger, ledger);
  }

  getInvestorsForProject(project_id: number): string[] {
    const rows = this.db
      .prepare(
        "SELECT DISTINCT investor_address FROM investor_projects WHERE project_id = ?",
      )
      .all(project_id) as { investor_address: string }[];
    return rows.map((r) => r.investor_address);
  }

  // ── Ledger tracking ───────────────────────────────────────────────────

  getLastProcessedLedger(): number {
    const row = this.db
      .prepare("SELECT MAX(ledger) as ledger FROM processed_ledgers")
      .get() as { ledger: number | null };
    return row.ledger ?? 0;
  }

  markLedgerProcessed(ledger: number): void {
    this.db
      .prepare("INSERT OR IGNORE INTO processed_ledgers (ledger) VALUES (?)")
      .run(ledger);
  }

  // ── Notification history ─────────────────────────────────────────────

  recordNotification(
    investor_address: string,
    project_id: number,
    channel: "email" | "webhook",
    ledger: number,
  ): void {
    this.db
      .prepare(
        `INSERT INTO notification_history
           (investor_address, project_id, channel, ledger)
         VALUES (?, ?, ?, ?)`,
      )
      .run(investor_address, project_id, channel, ledger);
  }

  /**
   * Check whether a specific (investor, project, ledger) notification has
   * already been recorded. Used for cross-restart dedup — the in-memory
   * Set is lost on process restart, but the DB table persists.
   */
  hasBeenNotified(
    investor_address: string,
    project_id: number,
    ledger: number,
  ): boolean {
    const row = this.db
      .prepare(
        `SELECT 1 FROM notification_history
         WHERE investor_address = ? AND project_id = ? AND ledger = ?
         LIMIT 1`,
      )
      .get(investor_address, project_id, ledger);
    return !!row;
  }

  /**
   * Returns a page of notification history, most recent first, optionally
   * filtered to a single investor. Bounded by `limit`/`offset` so callers
   * can never pull the full unbounded history in one request.
   */
  listNotificationHistory(options: {
    investor_address?: string;
    limit: number;
    offset: number;
  }): NotificationHistoryPage {
    const { investor_address, limit, offset } = options;
    const where = investor_address ? "WHERE investor_address = ?" : "";
    const params = investor_address ? [investor_address] : [];

    const total = (
      this.db
        .prepare(`SELECT COUNT(*) as count FROM notification_history ${where}`)
        .get(...params) as { count: number }
    ).count;

    const rows = this.db
      .prepare(
        `SELECT * FROM notification_history ${where}
         ORDER BY id DESC
         LIMIT ? OFFSET ?`,
      )
      .all(...params, limit, offset) as Record<string, unknown>[];

    return {
      items: rows.map((r) => this.rowToHistoryEntry(r)),
      total,
      limit,
      offset,
    };
  }

  /** Returns true if the database connection is alive and responsive. */
  isHealthy(): boolean {
    try {
      this.db.prepare("SELECT 1").get();
      return true;
    } catch {
      return false;
    }
  }

  close(): void {
    this.db.close();
  }

  // ── Helpers ───────────────────────────────────────────────────────────

  private rowToPreference(
    row: Record<string, unknown>,
  ): NotificationPreference {
    return {
      investor_address: row.investor_address as string,
      email: (row.email as string) || undefined,
      webhook_url: (row.webhook_url as string) || undefined,
      enabled: Boolean(row.enabled),
      min_delta: row.min_delta as number,
      updated_at: row.updated_at as string,
    };
  }

  private rowToHistoryEntry(
    row: Record<string, unknown>,
  ): NotificationHistoryEntry {
    return {
      id: row.id as number,
      investor_address: row.investor_address as string,
      project_id: row.project_id as number,
      channel: row.channel as "email" | "webhook",
      ledger: row.ledger as number,
      sent_at: row.sent_at as string,
    };
  }
}
