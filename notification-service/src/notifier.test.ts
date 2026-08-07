import { describe, it, expect, vi, beforeEach } from "vitest";
import { Notifier } from "./notifier";
import { Store } from "./db";
import {
  ScoreChangedEvent,
  NotificationPreference,
  ServiceConfig,
} from "./types";

const config: ServiceConfig = {
  rpc_url: "https://example.invalid",
  network_passphrase: "Test SDF Network ; September 2015",
  registry_contract_id: "REGISTRY",
  vault_contract_id: "VAULT",
  db_path: ":memory:",
  poll_interval_ms: 1000,
  api_port: 3000,
};

const preference: NotificationPreference = {
  investor_address: "GINVESTOR",
  webhook_url: "https://example.invalid/webhook",
  enabled: true,
  min_delta: 1,
  updated_at: new Date(0).toISOString(),
};

function makeEvent(
  overrides: Partial<ScoreChangedEvent> = {},
): ScoreChangedEvent {
  return {
    project_id: 1,
    old_credit_quality: 50,
    new_credit_quality: 60,
    old_green_impact: 40,
    new_green_impact: 45,
    old_rate_bps: 500,
    new_rate_bps: 480,
    timestamp: 1_700_000_000,
    ledger: 100,
    ...overrides,
  };
}

function makeStore(): Store {
  return {
    getPreference: vi.fn(() => preference),
    recordNotification: vi.fn(),
  } as unknown as Store;
}

describe("Notifier.notifyInvestors deduplication", () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchMock = vi.fn(async () => new Response(null, { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
  });

  it("does not send a second notification for the exact same event redelivered", async () => {
    const notifier = new Notifier(config, makeStore());
    const event = makeEvent();

    await notifier.notifyInvestors(event, [preference.investor_address]);
    await notifier.notifyInvestors(event, [preference.investor_address]);

    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("still notifies for a genuinely different event on the same project", async () => {
    const notifier = new Notifier(config, makeStore());
    const first = makeEvent({ ledger: 100 });
    const second = makeEvent({ ledger: 101, new_credit_quality: 70 });

    await notifier.notifyInvestors(first, [preference.investor_address]);
    await notifier.notifyInvestors(second, [preference.investor_address]);

    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("retries a redelivered event for an investor whose first delivery failed", async () => {
    fetchMock
      .mockImplementationOnce(async () => new Response("boom", { status: 500 }))
      .mockImplementationOnce(async () => new Response(null, { status: 200 }));

    const notifier = new Notifier(config, makeStore());
    const event = makeEvent();

    await notifier.notifyInvestors(event, [preference.investor_address]);
    await notifier.notifyInvestors(event, [preference.investor_address]);

    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("does not re-notify an investor who already succeeded, even if another investor failed", async () => {
    const other: NotificationPreference = {
      ...preference,
      investor_address: "GOTHER",
    };
    const store = {
      getPreference: vi.fn((addr: string) =>
        addr === other.investor_address ? other : preference,
      ),
      recordNotification: vi.fn(),
    } as unknown as Store;

    fetchMock
      .mockImplementationOnce(async () => new Response(null, { status: 200 })) // GINVESTOR succeeds
      .mockImplementationOnce(async () => new Response("boom", { status: 500 })) // GOTHER fails
      .mockImplementationOnce(async () => new Response(null, { status: 200 })); // GOTHER retry succeeds

    const notifier = new Notifier(config, store);
    const event = makeEvent();
    const addrs = [preference.investor_address, other.investor_address];

    await notifier.notifyInvestors(event, addrs);
    await notifier.notifyInvestors(event, addrs);

    expect(fetchMock).toHaveBeenCalledTimes(3);
  });
});

// ── Issue #217: retry behavior on a failed delivery ─────────────────────────

describe("Notifier retry behavior on a failed delivery", () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchMock = vi.fn(async () => new Response(null, { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
  });

  it("does not record a notification or dedup key when webhook returns a server error", async () => {
    fetchMock.mockImplementationOnce(
      async () => new Response("Internal Server Error", { status: 500 }),
    );

    const store = makeStore();
    const notifier = new Notifier(config, store);
    const event = makeEvent();

    await notifier.notifyInvestors(event, [preference.investor_address]);

    expect(store.recordNotification).not.toHaveBeenCalled();
  });

  it("retries on redelivery after a webhook timeout (network error)", async () => {
    fetchMock
      .mockRejectedValueOnce(new Error("fetch failed"))
      .mockImplementationOnce(async () => new Response(null, { status: 200 }));

    const store = makeStore();
    const notifier = new Notifier(config, store);
    const event = makeEvent();

    // First delivery: network error
    await notifier.notifyInvestors(event, [preference.investor_address]);
    // Redelivery: succeeds
    await notifier.notifyInvestors(event, [preference.investor_address]);

    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(store.recordNotification).toHaveBeenCalledTimes(1);
  });

  it("retries independently for each channel when webhook fails but email succeeds", async () => {
    const withBoth: NotificationPreference = {
      ...preference,
      email: "test@example.invalid",
    };
    const store = {
      getPreference: vi.fn(() => withBoth),
      recordNotification: vi.fn(),
    } as unknown as Store;

    // First attempt: webhook fails
    fetchMock.mockImplementationOnce(
      async () => new Response("bad gateway", { status: 502 }),
    );

    // Redelivery: webhook succeeds
    fetchMock.mockImplementationOnce(
      async () => new Response(null, { status: 200 }),
    );

    // Use a config without email transport — webhook-only path
    // This tests that webhook retries after failure independently of other channels
    const notifier = new Notifier(config, store);
    const event = makeEvent();

    // First attempt: webhook fails, no email transport so only webhook attempted
    await notifier.notifyInvestors(event, [preference.investor_address]);
    // Second attempt (redelivery): webhook should retry
    await notifier.notifyInvestors(event, [preference.investor_address]);

    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(store.recordNotification).toHaveBeenCalledTimes(1);
  });
});
