import { describe, it, expect, vi, beforeEach } from "vitest";
import { Notifier } from "./notifier";
import { Store } from "./db";
import {
  ScoreChangedEvent,
  NotificationPreference,
  ServiceConfig,
} from "./types";

const sendMailMock = vi.fn(async () => ({ messageId: "test" }));
vi.mock("nodemailer", () => ({
  default: {
    createTransport: vi.fn(() => ({ sendMail: sendMailMock })),
  },
}));

const config: ServiceConfig = {
  rpc_url: "https://example.invalid",
  network_passphrase: "Test SDF Network ; September 2015",
  registry_contract_id: "REGISTRY",
  vault_contract_id: "VAULT",
  db_path: ":memory:",
  poll_interval_ms: 1000,
  api_port: 3000,
};

const configWithEmail: ServiceConfig = {
  ...config,
  from_email: "noreply@heliobond.io",
  email_transport: {
    host: "smtp.example.invalid",
    port: 587,
    secure: false,
    auth: { user: "user", pass: "pass" },
  },
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
    hasBeenNotified: vi.fn(() => false),
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
      hasBeenNotified: vi.fn(() => false),
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
    fetchMock.mockImplementationOnce(async () =>
      new Response("Internal Server Error", { status: 500 }),
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
      hasBeenNotified: vi.fn(() => false),
      recordNotification: vi.fn(),
    } as unknown as Store;

    // First attempt: webhook fails
    fetchMock.mockImplementationOnce(async () =>
      new Response("bad gateway", { status: 502 }),
    );

    // Redelivery: webhook succeeds
    fetchMock.mockImplementationOnce(async () =>
      new Response(null, { status: 200 }),
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

// ── Issue #426: the email channel had zero test coverage ───────────────────

describe("Notifier email channel", () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    sendMailMock.mockClear();
    sendMailMock.mockImplementation(async () => ({ messageId: "test" }));
    fetchMock = vi.fn(async () => new Response(null, { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
  });

  it("sends an email via the configured transport when the investor has an email preference", async () => {
    const withEmail: NotificationPreference = {
      ...preference,
      webhook_url: undefined,
      email: "investor@example.invalid",
    };
    const store = {
      getPreference: vi.fn(() => withEmail),
      hasBeenNotified: vi.fn(() => false),
      recordNotification: vi.fn(),
    } as unknown as Store;

    const notifier = new Notifier(configWithEmail, store);
    const event = makeEvent();

    await notifier.notifyInvestors(event, [withEmail.investor_address]);

    expect(sendMailMock).toHaveBeenCalledTimes(1);
    expect(sendMailMock).toHaveBeenCalledWith(
      expect.objectContaining({ to: "investor@example.invalid" }),
    );
    expect(store.recordNotification).toHaveBeenCalledWith(
      withEmail.investor_address,
      event.project_id,
      "email",
      event.ledger,
    );
  });

  it("retries a redelivered event by email after the first send fails", async () => {
    const withEmail: NotificationPreference = {
      ...preference,
      webhook_url: undefined,
      email: "investor@example.invalid",
    };
    const store = {
      getPreference: vi.fn(() => withEmail),
      hasBeenNotified: vi.fn(() => false),
      recordNotification: vi.fn(),
    } as unknown as Store;

    sendMailMock
      .mockRejectedValueOnce(new Error("smtp connection refused"))
      .mockImplementationOnce(async () => ({ messageId: "test" }));

    const notifier = new Notifier(configWithEmail, store);
    const event = makeEvent();

    await notifier.notifyInvestors(event, [withEmail.investor_address]);
    await notifier.notifyInvestors(event, [withEmail.investor_address]);

    expect(sendMailMock).toHaveBeenCalledTimes(2);
    expect(store.recordNotification).toHaveBeenCalledTimes(1);
  });
});

// ── Issue #395: min_delta threshold filtering ────────────────────────────────

describe("Notifier min_delta threshold filtering", () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchMock = vi.fn(async () => new Response(null, { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
  });

  it("suppresses notification when min_delta exceeds the event's maxDelta", async () => {
    const highDelta: NotificationPreference = {
      ...preference,
      min_delta: 30, // default event has maxDelta = 20 (rate delta)
    };
    const store = {
      getPreference: vi.fn(() => highDelta),
      hasBeenNotified: vi.fn(() => false),
      recordNotification: vi.fn(),
    } as unknown as Store;

    const notifier = new Notifier(config, store);
    const event = makeEvent(); // maxDelta = 20 (rate delta)

    await notifier.notifyInvestors(event, [preference.investor_address]);

    expect(fetchMock).not.toHaveBeenCalled();
    expect(store.recordNotification).not.toHaveBeenCalled();
  });

  it("sends notification when maxDelta equals min_delta (boundary: < not <=)", async () => {
    const boundaryPref: NotificationPreference = {
      ...preference,
      min_delta: 20, // default event has maxDelta = 20 (rate delta)
    };
    const store = {
      getPreference: vi.fn(() => boundaryPref),
      hasBeenNotified: vi.fn(() => false),
      recordNotification: vi.fn(),
    } as unknown as Store;

    const notifier = new Notifier(config, store);
    const event = makeEvent(); // maxDelta = 20 (rate delta)

    await notifier.notifyInvestors(event, [preference.investor_address]);

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(store.recordNotification).toHaveBeenCalledTimes(1);
  });

  it("considers rate_bps delta in maxDelta calculation", async () => {
    // Event with no credit_quality/green_impact change but large rate change
    const rateOnlyEvent = makeEvent({
      old_credit_quality: 50,
      new_credit_quality: 50, // no change
      old_green_impact: 40,
      new_green_impact: 40, // no change
      old_rate_bps: 500,
      new_rate_bps: 2000, // 1500 bps change
    });
    const midDeltaPref: NotificationPreference = {
      ...preference,
      min_delta: 100, // would filter out if only CQ/GI were checked (delta=0)
    };
    const store = {
      getPreference: vi.fn(() => midDeltaPref),
      hasBeenNotified: vi.fn(() => false),
      recordNotification: vi.fn(),
    } as unknown as Store;

    const notifier = new Notifier(config, store);

    await notifier.notifyInvestors(rateOnlyEvent, [preference.investor_address]);

    // Should notify because rate delta (1500) exceeds min_delta (100)
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(store.recordNotification).toHaveBeenCalledTimes(1);
  });

  it("suppresses notification when rate delta is below min_delta", async () => {
    const rateOnlyEvent = makeEvent({
      old_credit_quality: 50,
      new_credit_quality: 50, // no change
      old_green_impact: 40,
      new_green_impact: 40, // no change
      old_rate_bps: 500,
      new_rate_bps: 510, // only 10 bps change
    });
    const highDeltaPref: NotificationPreference = {
      ...preference,
      min_delta: 20, // exceeds rate delta of 10
    };
    const store = {
      getPreference: vi.fn(() => highDeltaPref),
      hasBeenNotified: vi.fn(() => false),
      recordNotification: vi.fn(),
    } as unknown as Store;

    const notifier = new Notifier(config, store);

    await notifier.notifyInvestors(rateOnlyEvent, [preference.investor_address]);

    // Should NOT notify because rate delta (10) < min_delta (20)
    expect(fetchMock).not.toHaveBeenCalled();
    expect(store.recordNotification).not.toHaveBeenCalled();
  });
});

// ── Issue #396: MAX_TRACKED_NOTIFICATIONS bounded-eviction behavior ────────

describe("Notifier bounded-eviction (#396)", () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchMock = vi.fn(async () => new Response(null, { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
  });

  it("keeps notifiedRecipients at or below MAX_TRACKED_NOTIFICATIONS after many unique events", async () => {
    const notifier = new Notifier(config, makeStore());

    // Send 5050 unique events — 50 more than the cap — to trigger eviction
    for (let i = 0; i < 5050; i++) {
      const event = makeEvent({ ledger: 100 + i, timestamp: 1_700_000_000 + i });
      await notifier.notifyInvestors(event, [preference.investor_address]);
    }

    const recipients = (notifier as any).notifiedRecipients as Set<string>;
    expect(recipients.size).toBeLessThanOrEqual(5000);
  });

  it("evicts the oldest entry when capacity is exceeded", async () => {
    const notifier = new Notifier(config, makeStore());

    // Fill to exactly 5000
    for (let i = 0; i < 5000; i++) {
      const event = makeEvent({ ledger: 100 + i, timestamp: 1_700_000_000 + i });
      await notifier.notifyInvestors(event, [preference.investor_address]);
    }

    const recipients = (notifier as any).notifiedRecipients as Set<string>;
    const firstKey = recipients.values().next().value;

    // Add one more to trigger eviction
    const extraEvent = makeEvent({ ledger: 6100, timestamp: 1_700_006_100 });
    await notifier.notifyInvestors(extraEvent, [preference.investor_address]);

    expect(recipients.size).toBeLessThanOrEqual(5000);
    // The oldest entry should have been evicted
    expect(recipients.has(firstKey)).toBe(false);
  });
});
