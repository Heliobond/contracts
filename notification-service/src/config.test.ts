import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { loadConfig } from "./config";

const ENV_KEYS = [
  "REGISTRY_CONTRACT_ID",
  "STELLAR_RPC_URL",
  "STELLAR_NETWORK_PASSPHRASE",
  "VAULT_CONTRACT_ID",
  "DB_PATH",
  "POLL_INTERVAL_MS",
  "FROM_EMAIL",
  "SMTP_HOST",
  "SMTP_PORT",
  "SMTP_SECURE",
  "SMTP_USER",
  "SMTP_PASS",
  "API_PORT",
] as const;

const originalEnv: Record<string, string | undefined> = {};

describe("loadConfig", () => {
  beforeEach(() => {
    for (const key of ENV_KEYS) {
      originalEnv[key] = process.env[key];
      delete process.env[key];
    }
  });

  afterEach(() => {
    for (const key of ENV_KEYS) {
      if (originalEnv[key] === undefined) delete process.env[key];
      else process.env[key] = originalEnv[key];
    }
  });

  it("throws a clear error when REGISTRY_CONTRACT_ID is missing", () => {
    expect(() => loadConfig()).toThrow(
      /Missing required environment variable\(s\): REGISTRY_CONTRACT_ID/,
    );
  });

  it("does not throw once REGISTRY_CONTRACT_ID is set", () => {
    process.env.REGISTRY_CONTRACT_ID = "CONTRACT123";
    expect(() => loadConfig()).not.toThrow();
  });

  it("applies defaults for optional values", () => {
    process.env.REGISTRY_CONTRACT_ID = "CONTRACT123";
    const config = loadConfig();

    expect(config.rpc_url).toBe("https://soroban-testnet.stellar.org");
    expect(config.network_passphrase).toBe("Test SDF Network ; September 2015");
    expect(config.db_path).toBe("./data/notifications.db");
    expect(config.poll_interval_ms).toBe(30000);
    expect(config.api_port).toBe(3000);
    expect(config.email_transport).toBeUndefined();
  });

  it("builds the email transport only when SMTP_HOST is set", () => {
    process.env.REGISTRY_CONTRACT_ID = "CONTRACT123";
    process.env.SMTP_HOST = "smtp.example.com";
    process.env.SMTP_PORT = "2525";
    process.env.SMTP_SECURE = "true";
    process.env.SMTP_USER = "user";
    process.env.SMTP_PASS = "pass";

    const config = loadConfig();

    expect(config.email_transport).toEqual({
      host: "smtp.example.com",
      port: 2525,
      secure: true,
      auth: { user: "user", pass: "pass" },
    });
  });
});
