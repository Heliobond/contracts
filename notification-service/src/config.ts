import { ServiceConfig } from "./types";

export function loadConfig(): ServiceConfig {
  return {
    rpc_url: process.env.STELLAR_RPC_URL || "https://soroban-testnet.stellar.org",
    network_passphrase:
      process.env.STELLAR_NETWORK_PASSPHRASE ||
      "Test SDF Network ; September 2015",
    registry_contract_id:
      process.env.REGISTRY_CONTRACT_ID || "",
    vault_contract_id:
      process.env.VAULT_CONTRACT_ID || "",
    db_path: process.env.DB_PATH || "./data/notifications.db",
    poll_interval_ms: parseInt(process.env.POLL_INTERVAL_MS || "30000", 10),
    from_email: process.env.FROM_EMAIL,
    email_transport: process.env.SMTP_HOST
      ? {
          host: process.env.SMTP_HOST,
          port: parseInt(process.env.SMTP_PORT || "587", 10),
          secure: process.env.SMTP_SECURE === "true",
          auth: {
            user: process.env.SMTP_USER || "",
            pass: process.env.SMTP_PASS || "",
          },
        }
      : undefined,
    api_port: parseInt(process.env.API_PORT || "3000", 10),
  };
}
