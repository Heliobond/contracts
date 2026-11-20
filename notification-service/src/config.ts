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

const MAINNET_PASSPHRASE = "Public Global Stellar Network ; September 2015";
const TESTNET_PASSPHRASE = "Test SDF Network ; September 2015";

/** Derives a short, human-readable network label from the network passphrase. */
export function networkLabel(passphrase: string): string {
  if (passphrase === MAINNET_PASSPHRASE) return "Mainnet";
  if (passphrase === TESTNET_PASSPHRASE) return "Testnet";
  return "Custom Network";
}

/**
 * Validates and normalises a portfolio base URL.
 * Throws at startup if the value is set but cannot be parsed as an http/https URL.
 */
function requirePortfolioBaseUrl(raw: string | undefined): string {
  const value = raw?.trim() || "https://heliobond.io";
  try {
    const parsed = new URL(value);
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
      throw new Error("must use http or https");
    }
  } catch {
    throw new Error(
      `PORTFOLIO_BASE_URL "${value}" is not a valid http/https URL`,
    );
  }
  // Strip a trailing slash so callers can consistently append "/portfolio".
  return value.replace(/\/+$/, "");
}

export function loadConfig(): ServiceConfig {
  assertRequiredEnvVars();

  const passphrase =
    process.env.STELLAR_NETWORK_PASSPHRASE || TESTNET_PASSPHRASE;

  return {
    rpc_url:
      process.env.STELLAR_RPC_URL || "https://soroban-testnet.stellar.org",
    network_passphrase: passphrase,
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
            ? {
                auth: {
                  user: process.env.SMTP_USER,
                  pass: process.env.SMTP_PASS,
                },
              }
            : {}),
        }
      : undefined,
    api_port: requirePositiveInt("API_PORT", process.env.API_PORT, 3000),
    portfolio_base_url: requirePortfolioBaseUrl(process.env.PORTFOLIO_BASE_URL),
    network_label: networkLabel(passphrase),
  };
}
