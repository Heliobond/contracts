/** Matches the on-chain ScoreChanged event from ProjectRegistry (#131). */
export interface ScoreChangedEvent {
  project_id: number;
  old_credit_quality: number;
  new_credit_quality: number;
  old_green_impact: number;
  new_green_impact: number;
  old_rate_bps: number;
  new_rate_bps: number;
  /** Block timestamp when the event was emitted. */
  timestamp: number;
  /** Stellar ledger sequence number. */
  ledger: number;
}

/** Investor notification preference. */
export interface NotificationPreference {
  /** Stellar public address of the investor. */
  investor_address: string;
  /** Email address for email notifications (optional). */
  email?: string;
  /** Webhook URL for HTTP POST notifications (optional). */
  webhook_url?: string;
  /** Whether score change notifications are enabled. */
  enabled: boolean;
  /** Minimum absolute score change required to trigger a notification (0-100). */
  min_delta: number;
  /** When this preference was created/updated. */
  updated_at: string;
}

/** Payload sent to webhook URLs. */
export interface WebhookPayload {
  event: "score_changed";
  project_id: number;
  old_scores: { credit_quality: number; green_impact: number };
  new_scores: { credit_quality: number; green_impact: number };
  old_rate_bps: number;
  new_rate_bps: number;
  investor_address: string;
  timestamp: string;
}

/** Database row for tracking which investors are in which projects. */
export interface InvestorProjectRow {
  investor_address: string;
  project_id: number;
  first_seen_ledger: number;
  last_seen_ledger: number;
}

/** Configuration for the notification service. */
export interface ServiceConfig {
  rpc_url: string;
  network_passphrase: string;
  registry_contract_id: string;
  vault_contract_id: string;
  db_path: string;
  poll_interval_ms: number;
  from_email?: string;
  email_transport?: {
    host: string;
    port: number;
    secure: boolean;
    auth: {
      user: string;
      pass: string;
    };
  };
  api_port: number;
}
