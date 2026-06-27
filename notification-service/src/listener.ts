import {
  SorobanRpc,
  Contract,
  scValToNative,
  xdr,
} from "@stellar/stellar-sdk";
import { ScoreChangedEvent, ServiceConfig } from "./types";

const SCORE_CHANGED_TOPIC = "ScoreChanged";

/** Decode a ScoreChanged event from a Soroban transaction event. */
function decodeScoreChanged(
  event: xdr.ContractEvent,
  ledger: number,
  timestamp: number,
): ScoreChangedEvent | null {
  try {
    const body = event.body();
    if (body.switch() !== xdr.ContractEventType.contractEventTypeV0) {
      return null;
    }
    const v0 = body.v0();
    const topics = v0.topics();

    // topics[0] = event name (Symbol), topics[1] = project_id (u32)
    if (topics.length() < 2) return null;
    const eventName = scValToNative(topics.get(0));
    if (eventName !== SCORE_CHANGED_TOPIC) return null;

    const projectId = Number(scValToNative(topics.get(1)));
    const data = scValToNative(v0.data());

    // data is {Vec: [old_cq, new_cq, old_gi, new_gi, old_rate, new_rate]}
    if (!Array.isArray(data)) return null;
    const [
      old_credit_quality,
      new_credit_quality,
      old_green_impact,
      new_green_impact,
      old_rate_bps,
      new_rate_bps,
    ] = data.map(Number);

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

/** Fetch events from Soroban RPC for a range of ledgers. */
async function fetchEvents(
  server: SorobanRpc.Server,
  contractId: string,
  startLedger: number,
  endLedger: number,
): Promise<ScoreChangedEvent[]> {
  const results: ScoreChangedEvent[] = [];

  try {
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
      },
    });

    for (const event of response.events) {
      if (!event.value) continue;
      const decoded = decodeScoreChanged(
        event.value,
        event.ledger,
        event.timestamp,
      );
      if (decoded) {
        results.push(decoded);
      }
    }
  } catch (err) {
    console.error("Error fetching events:", err);
  }

  return results;
}

/** Poll for new ScoreChanged events and invoke the callback. */
export async function pollScoreChanges(
  config: ServiceConfig,
  onEvent: (event: ScoreChangedEvent) => Promise<void>,
  getLastLedger: () => Promise<number>,
  setLastLedger: (ledger: number) => Promise<void>,
): Promise<void> {
  const server = new SorobanRpc.Server(config.rpc_url);

  console.log(
    `[listener] Starting poll every ${config.poll_interval_ms}ms for contract ${config.registry_contract_id}`,
  );

  const poll = async () => {
    try {
      const latestLedger = await server.getLatestLedger();
      const lastProcessed = await getLastLedger();
      const startLedger = lastProcessed > 0 ? lastProcessed + 1 : latestLedger.sequence;

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

  // Initial poll, then interval
  await poll();
  setInterval(poll, config.poll_interval_ms);
}
