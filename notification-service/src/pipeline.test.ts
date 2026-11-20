import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { xdr, nativeToScVal } from "@stellar/stellar-sdk";
import { decodeScoreChanged } from "./listener";
import { Notifier } from "./notifier";
import { Store } from "./db";
import { ServiceConfig } from "./types";

const LEDGER = 12345;
const TIMESTAMP = 1_700_000_000;

// Same fixture-building approach as listener.test.ts: the SDK's public
// .d.ts doesn't expose named factories for these XDR unions, so we go
// through `any` to construct a realistic ContractEvent.
function unionOf<T>(Ctor: unknown, ...args: unknown[]): T {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return new (Ctor as any)(...args);
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

/** Builds a synthetic on-chain ScoreChanged contract event, as the listener would receive it from Soroban RPC. */
function buildScoreChangedEvent(projectId: number): xdr.ContractEvent {
  const topics = [
    nativeToScVal("score_changed", { type: "symbol" }),
    nativeToScVal(projectId, { type: "u32" }),
  ];
  const data = buildDataMap({
    old_credit_quality: 50,
    new_credit_quality: 80,
    old_green_impact: 40,
    new_green_impact: 60,
    old_rate_bps: 750,
    new_rate_bps: 650,
  });

  const v0 = new xdr.ContractEventV0({ topics, data });
  const body = unionOf<xdr.ContractEventBody>(xdr.ContractEventBody, 0, v0);
  const ext = unionOf<xdr.ExtensionPoint>(xdr.ExtensionPoint, 0, undefined);

  return new xdr.ContractEvent({
    ext,
    contractId: null,
    type: xdr.ContractEventType.contract(),
    body,
  });
}

const config: ServiceConfig = {
  rpc_url: "https://example.invalid",
  network_passphrase: "Test SDF Network ; September 2015",
  registry_contract_id: "REGISTRY",
  vault_contract_id: "VAULT",
  db_path: ":memory:",
  poll_interval_ms: 1000,
  api_port: 3000,
};

describe("listener -> notifier pipeline", () => {
  let store: Store;
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    store = new Store(":memory:");
    fetchMock = vi.fn(async () => new Response(null, { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
  });

  afterEach(() => {
    store.close();
    vi.unstubAllGlobals();
  });

  it("decodes a synthetic on-chain event and notifies subscribed investors via webhook", async () => {
    const investor = "GINVESTORADDRESS";
    store.upsertPreference({
      investor_address: investor,
      email: undefined,
      webhook_url: "https://example.invalid/webhook",
      enabled: true,
      min_delta: 1,
      updated_at: new Date(0).toISOString(),
    });
    store.recordInvestment(investor, 7, LEDGER);

    const rawEvent = buildScoreChangedEvent(7);
    const decoded = decodeScoreChanged(rawEvent, LEDGER, TIMESTAMP);
    expect(decoded).not.toBeNull();

    const notifier = new Notifier(config, store);
    const investors = store.getInvestorsForProject(decoded!.project_id);
    expect(investors).toEqual([investor]);

    await notifier.notifyInvestors(decoded!, investors);

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, requestInit] = fetchMock.mock.calls[0];
    expect(url).toBe("https://example.invalid/webhook");
    const payload = JSON.parse(requestInit.body as string);
    expect(payload).toMatchObject({
      event: "score_changed",
      project_id: 7,
      old_scores: { credit_quality: 50, green_impact: 40 },
      new_scores: { credit_quality: 80, green_impact: 60 },
      old_rate_bps: 750,
      new_rate_bps: 650,
      investor_address: investor,
    });

    const history = store.listNotificationHistory({ limit: 10, offset: 0 });
    expect(history.total).toBe(1);
    expect(history.items[0]).toMatchObject({
      investor_address: investor,
      project_id: 7,
      channel: "webhook",
      ledger: LEDGER,
    });
  });

  it("does not notify investors who are not indexed for the affected project", async () => {
    const investor = "GUNRELATEDINVESTOR";
    store.upsertPreference({
      investor_address: investor,
      email: undefined,
      webhook_url: "https://example.invalid/webhook",
      enabled: true,
      min_delta: 1,
      updated_at: new Date(0).toISOString(),
    });
    // Investor holds a stake in project 1, but the event is for project 7.
    store.recordInvestment(investor, 1, LEDGER);

    const rawEvent = buildScoreChangedEvent(7);
    const decoded = decodeScoreChanged(rawEvent, LEDGER, TIMESTAMP);
    expect(decoded).not.toBeNull();

    const notifier = new Notifier(config, store);
    const investors = store.getInvestorsForProject(decoded!.project_id);
    expect(investors).toEqual([]);

    await notifier.notifyInvestors(decoded!, investors);

    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("skips investors whose min_delta exceeds the actual score change", async () => {
    const investor = "GPICKYINVESTOR";
    store.upsertPreference({
      investor_address: investor,
      email: undefined,
      webhook_url: "https://example.invalid/webhook",
      enabled: true,
      min_delta: 1000, // higher than any possible delta
      updated_at: new Date(0).toISOString(),
    });
    store.recordInvestment(investor, 7, LEDGER);

    const rawEvent = buildScoreChangedEvent(7);
    const decoded = decodeScoreChanged(rawEvent, LEDGER, TIMESTAMP);

    const notifier = new Notifier(config, store);
    const investors = store.getInvestorsForProject(decoded!.project_id);

    await notifier.notifyInvestors(decoded!, investors);

    expect(fetchMock).not.toHaveBeenCalled();
    expect(store.listNotificationHistory({ limit: 10, offset: 0 }).total).toBe(
      0,
    );
  });
});
