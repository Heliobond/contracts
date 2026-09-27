import { SorobanRpc, scValToNative, xdr } from "@stellar/stellar-sdk";
import { ScoreChangedEvent, ServiceConfig } from "./types";

// The #[contractevent] macro derives the prefix topic from the struct name in
// snake_case by default (ScoreChanged -> "score_changed") — see
// project_registry/src/events.rs and EVENTS.md.
const SCORE_CHANGED_TOPIC = "score_changed";

/** Non-#[topic] fields of ScoreChanged, as published (default data_format = Map). */
const SCORE_CHANGED_DATA_FIELDS = [
  "old_credit_quality",
  "new_credit_quality",
  "old_green_impact",
  "new_green_impact",
  "old_rate_bps",
  "new_rate_bps",
] as const;

/** Decode a ScoreChanged event from a Soroban transaction event. */
export function decodeScoreChanged(
  event: xdr.ContractEvent,
  ledger: number,
  timestamp: number,
  expectedContractId?: string,
): ScoreChangedEvent | null {
  try {
    // When an expected contract ID is provided, verify the event originated
    // from that contract. This is a defense-in-depth check — the RPC filter
    // in fetchEvents already scopes by contractId, but rejecting here too
    // means a mis-routed or spoofed event can never reach notification
    // delivery even if the upstream filter is bypassed.
    if (expectedContractId !== undefined) {
      const raw = event.contractId();
      if (!raw) return null;
      const eventContractId = Buffer.from(raw).toString("hex");
      if (eventContractId !== expectedContractId) return null;
    }

    const body = event.body();
    // ContractEventBody.switch() is the raw union discriminant (0 = v0, the only
    // arm currently defined by the XDR spec); the SDK doesn't expose a named
    // enum for it, so we compare against the literal directly.
    if (body.switch() !== 0) {
      return null;
    }
    const v0 = body.v0();
    const topics = v0.topics();

    // topics[0] = event name (Symbol), topics[1] = project_id (u32) — only
    // `project_id` is marked #[topic] on the ScoreChanged struct.
    if (topics.length < 2) return null;
    const eventName = scValToNative(topics[0]);
    if (eventName !== SCORE_CHANGED_TOPIC) return null;

    const projectId = Number(scValToNative(topics[1]));

    // The remaining (non-topic) fields are published as a Map keyed by field
    // name (the macro's default data_format), not a positional vector — see
    // derive_event.rs's DataFormat::Map branch. scValToNative turns that into
    // a plain object keyed by the Rust field names.
    const data = scValToNative(v0.data());
    if (typeof data !== "object" || data === null || Array.isArray(data)) {
      return null;
    }

    const fields = data as Record<string, unknown>;
    // All fields are required (non-optional) on ScoreChangedEvent, so reject
    // anything that doesn't decode to a finite number instead of letting
    // undefined/NaN through.
    const values = SCORE_CHANGED_DATA_FIELDS.map((key) => Number(fields[key]));
    if (![projectId, ...values].every(Number.isFinite)) {
      return null;
    }
    const [
      old_credit_quality,
      new_credit_quality,
      old_green_impact,
      new_green_impact,
      old_rate_bps,
      new_rate_bps,
    ] = values;

    return {
      project_id: projectId,
      old_credit_quality,
      new_credit_quality,
      old_green_impact,
      new_green_impact,
      old_rate_bps,
      new_rate_bps,
      timestamp,
      ledger,
    };
  } catch {
    return null;
  }
}

/** Fetch events from Soroban RPC for a range of ledgers, paginating through all results. */
async function fetchEvents(
  server: SorobanRpc.Server,
  contractId: string,
  startLedger: number,
  _endLedger: number,
): Promise<ScoreChangedEvent[]> {
  const results: ScoreChangedEvent[] = [];
  let cursor: string | undefined;

  do {
    const response = await server.getEvents({
      startLedger,
      filters: [
        {
          contractId,
          type: "contract",
        },
      ],
      pagination: {
        limit: 100,
        ...(cursor ? { cursor } : {}),
      },
    });

    for (const event of response.events) {
      if (!event.value) continue;
      const decoded = decodeScoreChanged(
        event.value,
        event.ledger,
        event.timestamp,
        contractId,
      );
      if (decoded) {
        console.log(
          `[listener] Processed event: type=score_changed contract=${contractId} ledger=${event.ledger} project=${decoded.project_id}`,
        );
        results.push(decoded);
      }
    }

    cursor = response.events.length === 100 ? response.cursor : undefined;
  } while (cursor);

  return results;
}

/** Handle returned by {@link pollScoreChanges} to stop polling gracefully. */
export interface PollHandle {
  /** Stop scheduling further polls and wait for any in-flight poll to finish. */
  stop(): Promise<void>;
}

/** Poll for new ScoreChanged events and invoke the callback. */
export async function pollScoreChanges(
  config: ServiceConfig,
  onEvent: (event: ScoreChangedEvent) => Promise<void>,
  getLastLedger: () => Promise<number>,
  setLastLedger: (ledger: number) => Promise<void>,
): Promise<PollHandle> {
  const server = new SorobanRpc.Server(config.rpc_url);

  // Sanity-check that rpc_url actually points at the network config claims to
  // be (#433): network_passphrase was parsed from config and documented but
  // never used anywhere, so a misconfigured RPC endpoint (e.g. prod pointed
  // at testnet) would previously go undetected until symptoms showed up
  // downstream. Fire-and-forget -- this must never delay or block the poll
  // loop starting up; a confirmed mismatch is just logged loudly.
  server
    .getNetwork()
    .then((network) => {
      if (network.passphrase !== config.network_passphrase) {
        console.error(
          `[listener] Network passphrase mismatch: rpc_url reports "${network.passphrase}" but config expects "${config.network_passphrase}". Check STELLAR_RPC_URL/STELLAR_NETWORK_PASSPHRASE.`,
        );
      }
    })
    .catch((err) => {
      console.error("[listener] Could not verify RPC network passphrase:", err);
    });

  console.log(
    `[listener] Starting poll every ${config.poll_interval_ms}ms for contract ${config.registry_contract_id}`,
  );

  let stopped = false;
  let timer: NodeJS.Timeout | undefined;
  let currentPoll: Promise<void> = Promise.resolve();

  const poll = async () => {
    try {
      const latestLedger = await server.getLatestLedger();
      const lastProcessed = await getLastLedger();
      const startLedger =
        lastProcessed > 0 ? lastProcessed + 1 : latestLedger.sequence;

      if (startLedger > latestLedger.sequence) {
        return; // caught up
      }

      console.log(
        `[listener] Scanning ledgers ${startLedger} → ${latestLedger.sequence}`,
      );

      const events = await fetchEvents(
        server,
        config.registry_contract_id,
        startLedger,
        latestLedger.sequence,
      );

      for (const ev of events) {
        try {
          await onEvent(ev);
        } catch (err) {
          console.error(
            `[listener] Error processing event for project ${ev.project_id}:`,
            err,
          );
        }
      }

      await setLastLedger(latestLedger.sequence);
    } catch (err) {
      console.error("[listener] Poll error:", err);
    }
  };

  const scheduleNext = () => {
    if (stopped) return;
    timer = setTimeout(() => {
      currentPoll = poll();
      currentPoll.then(scheduleNext);
    }, config.poll_interval_ms);
  };

  // Kick off the initial poll but don't block on it — `stop()` already
  // tracks it via `currentPoll`, so the handle (and therefore signal-based
  // graceful shutdown) is available immediately rather than only after a
  // full RPC round-trip, closing the window where a SIGTERM/SIGINT during
  // startup would bypass cleanup entirely.
  currentPoll = poll();
  currentPoll.then(scheduleNext);

  return {
    async stop(): Promise<void> {
      if (stopped) return;
      stopped = true;
      if (timer) clearTimeout(timer);
      await currentPoll;
    },
  };
}
