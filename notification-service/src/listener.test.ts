import { describe, it, expect, vi, beforeEach } from "vitest";
import type { ServiceConfig, ScoreChangedEvent } from "./types";

// Hoisted mock references — accessible before module evaluation
const { getLatestLedgerMock, getEventsMock, getNetworkMock } = vi.hoisted(
  () => ({
    getLatestLedgerMock: vi.fn(),
    getEventsMock: vi.fn(),
    getNetworkMock: vi.fn(),
  }),
);

vi.mock("@stellar/stellar-sdk", async (importOriginal) => {
  const mod = await importOriginal<typeof import("@stellar/stellar-sdk")>();
  return {
    ...mod,
    SorobanRpc: {
      ...mod.SorobanRpc,
      Server: vi.fn().mockImplementation(() => ({
        getLatestLedger: getLatestLedgerMock,
        getEvents: getEventsMock,
        getNetwork: getNetworkMock,
      })),
    },
  };
});

// Static imports pick up the mocked module
import { Contract, StrKey, xdr, nativeToScVal } from "@stellar/stellar-sdk";
import { decodeScoreChanged, pollScoreChanges } from "./listener";

const LEDGER = 12345;
const TIMESTAMP = 1_700_000_000;

// The SDK's public .d.ts only exposes named static factories for XDR union
// arms, but ContractEventBody/ExtensionPoint have no named arm — their real
// (runtime) constructor still takes (switch, value), so we go through `any`
// to build valid test fixtures without fighting the incomplete types.
function unionOf<T>(Ctor: unknown, ...args: unknown[]): T {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return new (Ctor as any)(...args);
}

const FULL_SCORES = {
  old_credit_quality: 50,
  new_credit_quality: 60,
  old_green_impact: 40,
  new_green_impact: 45,
  old_rate_bps: 500,
  new_rate_bps: 480,
};

/**
 * Mirrors what `#[contractevent]` actually publishes for `ScoreChanged`
 * (project_registry/src/events.rs): only `project_id` is `#[topic]`, so
 * topics are `[Symbol("score_changed"), project_id]`; the remaining fields
 * default to `data_format = "map"`, i.e. an ScMap keyed by field name — not
 * a positional vector. See EVENTS.md and derive_event.rs's DataFormat::Map
 * branch for the encoding this fixture reproduces.
 */
function buildScoreChangedEvent(
  topicValues: unknown[],
  data: xdr.ScVal,
  contractId?: Buffer,
): xdr.ContractEvent {
  const topics = topicValues.map((v) =>
    typeof v === "string"
      ? nativeToScVal(v, { type: "symbol" })
      : nativeToScVal(v, { type: "u32" }),
  );

  const v0 = new xdr.ContractEventV0({ topics, data });
  const body = unionOf<xdr.ContractEventBody>(xdr.ContractEventBody, 0, v0);
  const ext = unionOf<xdr.ExtensionPoint>(xdr.ExtensionPoint, 0, undefined);

  return new xdr.ContractEvent({
    ext,
    contractId: contractId ?? null,
    type: xdr.ContractEventType.contract(),
    body,
  });
}

function buildDataMap(fields: Record<string, number>): xdr.ScVal {
  const entries = Object.entries(fields)
    .sort(([a], [b]) => a.localeCompare(b))
    .map(
      ([key, value]) =>
        new xdr.ScMapEntry({
          key: nativeToScVal(key, { type: "symbol" }),
          val: nativeToScVal(value, { type: "u32" }),
        }),
    );
  return xdr.ScVal.scvMap(entries);
}

describe("decodeScoreChanged", () => {
  it("decodes a well-formed ScoreChanged event (Map data, snake_case topic)", () => {
    const event = buildScoreChangedEvent(
      ["score_changed", 7],
      buildDataMap(FULL_SCORES),
    );

    const decoded = decodeScoreChanged(event, LEDGER, TIMESTAMP);

    expect(decoded).toEqual({
      project_id: 7,
      ...FULL_SCORES,
      timestamp: TIMESTAMP,
      ledger: LEDGER,
    });
  });

  it("returns null when the event name doesn't match", () => {
    const event = buildScoreChangedEvent(
      ["some_other_event", 7],
      buildDataMap(FULL_SCORES),
    );
    expect(decodeScoreChanged(event, LEDGER, TIMESTAMP)).toBeNull();
  });

  it("returns null when fewer than 2 topics are present", () => {
    const event = buildScoreChangedEvent(
      ["score_changed"],
      buildDataMap(FULL_SCORES),
    );
    expect(decodeScoreChanged(event, LEDGER, TIMESTAMP)).toBeNull();
  });

  it("returns null when a required data field is missing instead of coercing to NaN", () => {
    const incomplete: Partial<typeof FULL_SCORES> = { ...FULL_SCORES };
    delete incomplete.new_rate_bps;
    const event = buildScoreChangedEvent(
      ["score_changed", 7],
      buildDataMap(incomplete),
    );
    expect(decodeScoreChanged(event, LEDGER, TIMESTAMP)).toBeNull();
  });

  it("returns null when data is a Vec instead of the expected Map", () => {
    const event = buildScoreChangedEvent(
      ["score_changed", 7],
      xdr.ScVal.scvVec(
        Object.values(FULL_SCORES).map((v) =>
          nativeToScVal(v, { type: "u32" }),
        ),
      ),
    );
    expect(decodeScoreChanged(event, LEDGER, TIMESTAMP)).toBeNull();
  });

  it("returns null when data is void", () => {
    const event = buildScoreChangedEvent(
      ["score_changed", 7],
      xdr.ScVal.scvVoid(),
    );
    expect(decodeScoreChanged(event, LEDGER, TIMESTAMP)).toBeNull();
  });

  it("accepts an event whose contractId matches the expected value", () => {
    const contractId = Buffer.alloc(32, 0xab);
    const event = buildScoreChangedEvent(
      ["score_changed", 7],
      buildDataMap(FULL_SCORES),
      contractId,
    );

    const decoded = decodeScoreChanged(
      event,
      LEDGER,
      TIMESTAMP,
      contractId.toString("hex"),
    );

    expect(decoded).toEqual({
      project_id: 7,
      ...FULL_SCORES,
      timestamp: TIMESTAMP,
      ledger: LEDGER,
    });
  });

  it("returns null when contractId does not match the expected value", () => {
    const eventContractId = Buffer.alloc(32, 0xab);
    const expectedContractId = Buffer.alloc(32, 0xcd).toString("hex");
    const event = buildScoreChangedEvent(
      ["score_changed", 7],
      buildDataMap(FULL_SCORES),
      eventContractId,
    );

    expect(
      decodeScoreChanged(event, LEDGER, TIMESTAMP, expectedContractId),
    ).toBeNull();
  });

  it("returns null when event has no contractId but one is expected", () => {
    const event = buildScoreChangedEvent(
      ["score_changed", 7],
      buildDataMap(FULL_SCORES),
    );

    expect(
      decodeScoreChanged(
        event,
        LEDGER,
        TIMESTAMP,
        Buffer.alloc(32, 0xab).toString("hex"),
      ),
    ).toBeNull();
  });
});

// ── Issue #216: reconnect after a dropped RPC connection ────────────────────

// Hex-encoded so it can round-trip through decodeScoreChanged's
// expectedContractId check (Buffer.from(event.contractId()).toString("hex")),
// now that fetchEvents actually passes it through (#432).
const TEST_REGISTRY_CONTRACT_ID_HEX =
  "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

describe("pollScoreChanges reconnects after a dropped RPC connection", () => {
  const config: ServiceConfig = {
    rpc_url: "https://example.invalid",
    network_passphrase: "Test SDF Network ; September 2015",
    registry_contract_id: TEST_REGISTRY_CONTRACT_ID_HEX,
    vault_contract_id: "VAULT",
    db_path: ":memory:",
    poll_interval_ms: 50,
    api_port: 3000,
  };

  beforeEach(() => {
    getLatestLedgerMock.mockReset();
    getEventsMock.mockReset();
    getNetworkMock.mockReset();
    // Default: server responds normally, caught up, and reports the network
    // config expects.
    getLatestLedgerMock.mockResolvedValue({ sequence: 100 });
    getEventsMock.mockResolvedValue({ events: [] });
    getNetworkMock.mockResolvedValue({
      passphrase: config.network_passphrase,
      protocolVersion: "22",
    });
  });

  it("continues polling after a connection error and processes subsequent events", async () => {
    const processed: ScoreChangedEvent[] = [];
    let ledger = 0;

    // First poll: connection drops
    getLatestLedgerMock
      .mockRejectedValueOnce(new Error("ECONNREFUSED"))
      // Second poll: succeeds
      .mockResolvedValueOnce({ sequence: 100 });

    // Second poll returns an event
    getEventsMock.mockResolvedValueOnce({
      events: [
        {
          contractId: new Contract(
            StrKey.encodeContract(
              Buffer.from(TEST_REGISTRY_CONTRACT_ID_HEX, "hex"),
            ),
          ),
          topic: [
            nativeToScVal("score_changed", { type: "symbol" }),
            nativeToScVal(7, { type: "u32" }),
          ],
          value: buildDataMap(FULL_SCORES),
          ledger: 100,
          ledgerClosedAt: new Date(TIMESTAMP * 1000).toISOString(),
          inSuccessfulContractCall: true,
          pagingToken: "100-1",
        },
      ],
    });

    const handle = await pollScoreChanges(
      config,
      async (ev) => {
        processed.push(ev);
      },
      async () => ledger,
      async (l) => {
        ledger = l;
      },
    );

    // Wait for two poll cycles (first fails at ~0ms, second fires at ~50ms)
    await new Promise((r) => setTimeout(r, config.poll_interval_ms * 5));
    await handle.stop();

    expect(getLatestLedgerMock.mock.calls.length).toBeGreaterThanOrEqual(2);
    expect(getEventsMock).toHaveBeenCalledWith({
      startLedger: 100,
      filters: [
        { contractIds: [config.registry_contract_id], type: "contract" },
      ],
      limit: 100,
    });
    expect(processed).toHaveLength(1);
    expect(processed[0].project_id).toBe(7);
  });
});

// ── Issue #433: network_passphrase must actually be checked against the RPC server ─

describe("pollScoreChanges network passphrase check", () => {
  const config: ServiceConfig = {
    rpc_url: "https://example.invalid",
    network_passphrase: "Test SDF Network ; September 2015",
    registry_contract_id: "REGISTRY",
    vault_contract_id: "VAULT",
    db_path: ":memory:",
    poll_interval_ms: 50,
    api_port: 3000,
  };

  beforeEach(() => {
    getLatestLedgerMock.mockReset();
    getEventsMock.mockReset();
    getNetworkMock.mockReset();
    getLatestLedgerMock.mockResolvedValue({ sequence: 100 });
    getEventsMock.mockResolvedValue({ events: [] });
  });

  it("logs an error when the RPC server's network passphrase doesn't match config", async () => {
    getNetworkMock.mockResolvedValue({
      passphrase: "Public Global Stellar Network ; September 2015",
      protocolVersion: "22",
    });
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    const handle = await pollScoreChanges(
      config,
      async () => {},
      async () => 0,
      async () => {},
    );
    // The network-passphrase check is fire-and-forget so it never delays the
    // poll loop starting; flush microtasks so its .then()/.catch() settles.
    await new Promise((r) => setTimeout(r, 0));
    await handle.stop();

    expect(getNetworkMock).toHaveBeenCalled();
    expect(
      errorSpy.mock.calls.some((call) =>
        String(call[0]).includes("Network passphrase mismatch"),
      ),
    ).toBe(true);

    errorSpy.mockRestore();
  });

  it("does not log a mismatch when the RPC server's network passphrase matches config", async () => {
    getNetworkMock.mockResolvedValue({
      passphrase: config.network_passphrase,
      protocolVersion: "22",
    });
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    const handle = await pollScoreChanges(
      config,
      async () => {},
      async () => 0,
      async () => {},
    );
    await new Promise((r) => setTimeout(r, 0));
    await handle.stop();

    expect(
      errorSpy.mock.calls.some((call) =>
        String(call[0]).includes("Network passphrase mismatch"),
      ),
    ).toBe(false);

    errorSpy.mockRestore();
  });
});
