import Database from "better-sqlite3";
import { NotificationPreference, InvestorProjectRow } from "./types";

export class Store {
  private db: Database.Database;

  constructor(path: string) {
    this.db = new Database(path);
    this.db.pragma("journal_mode = WAL");
    this.migrate();
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

      CREATE INDEX IF NOT EXISTS idx_investor_projects_project
        ON investor_projects(project_id);
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
      .prepare("SELECT * FROM notification_preferences WHERE investor_address = ?")
      .get(address) as Record<string, unknown> | undefined;
    if (!row) return undefined;
    return this.rowToPreference(row);
  }

  getAllEnabledPreferences(): NotificationPreference[] {
    const rows = this.db
      .prepare(
        "SELECT * FROM notification_preferences WHERE enabled = 1 AND (email IS NOT NULL OR webhook_url IS NOT NULL)",
      )
      .all() as Record<string, unknown>[];
    return rows.map((r) => this.rowToPreference(r));
  }

  listPreferences(): NotificationPreference[] {
    const rows = this.db
      .prepare("SELECT * FROM notification_preferences ORDER BY updated_at DESC")
      .all() as Record<string, unknown>[];
    return rows.map((r) => this.rowToPreference(r));
  }

  deletePreference(address: string): void {
    this.db
      .prepare("DELETE FROM notification_preferences WHERE investor_address = ?")
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

  close(): void {
    this.db.close();
  }

  // ── Helpers ───────────────────────────────────────────────────────────

  private rowToPreference(row: Record<string, unknown>): NotificationPreference {
    return {
      investor_address: row.investor_address as string,
      email: (row.email as string) || undefined,
      webhook_url: (row.webhook_url as string) || undefined,
      enabled: Boolean(row.enabled),
      min_delta: row.min_delta as number,
      updated_at: row.updated_at as string,
    };
  }
}
