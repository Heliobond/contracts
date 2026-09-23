import { ServiceConfig } from "./types";

/** Environment variables that must be set for the service to start. */
const REQUIRED_ENV_VARS = ["REGISTRY_CONTRACT_ID"] as const;

/**
 * Throws a clear, actionable error if any required environment variable is
 * missing, so the service fails fast at startup instead of misbehaving
 * later (e.g. polling an empty contract id).
 */
function assertRequiredEnvVars(): void {
  const missing = REQUIRED_ENV_VARS.filter((name) => !process.env[name]);
  if (missing.length > 0) {
    throw new Error(
      `Missing required environment variable(s): ${missing.join(", ")}`,
    );
  }
}

/**
 * Parses a numeric environment variable and throws if the value is set but
 * not a finite positive number. Returns the parsed value or the default.
 */
function requirePositiveInt(
  name: string,
  raw: string | undefined,
  defaultValue: number,
): number {
  if (raw === undefined || raw === "") return defaultValue;
  const n = Number(raw);
  if (!Number.isFinite(n) || n <= 0 || !Number.isInteger(n)) {
    throw new Error(
      `Environment variable ${name} must be a positive integer, got "${raw}"`,
    );
  }
  return n;
}

export function loadConfig(): ServiceConfig {
  assertRequiredEnvVars();

  return {
    rpc_url:
      process.env.STELLAR_RPC_URL || "https://soroban-testnet.stellar.org",
    network_passphrase:
      process.env.STELLAR_NETWORK_PASSPHRASE ||
      "Test SDF Network ; September 2015",
    registry_contract_id: process.env.REGISTRY_CONTRACT_ID || "",
    vault_contract_id: process.env.VAULT_CONTRACT_ID || "",
    db_path: process.env.DB_PATH || "./data/notifications.db",
    poll_interval_ms: requirePositiveInt(
      "POLL_INTERVAL_MS",
      process.env.POLL_INTERVAL_MS,
      30000,
    ),
    from_email: process.env.FROM_EMAIL,
    email_transport: process.env.SMTP_HOST
      ? {
          host: process.env.SMTP_HOST,
          port: requirePositiveInt("SMTP_PORT", process.env.SMTP_PORT, 587),
          secure: process.env.SMTP_SECURE === "true",
          ...(process.env.SMTP_USER && process.env.SMTP_PASS
            ? { auth: { user: process.env.SMTP_USER, pass: process.env.SMTP_PASS } }
            : {}),
        }
      : undefined,
    api_port: requirePositiveInt("API_PORT", process.env.API_PORT, 3000),
  };
}
