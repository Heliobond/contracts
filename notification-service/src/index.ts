import type { Server } from "http";
import { loadConfig } from "./config";
import { Store } from "./db";
import { Notifier } from "./notifier";
import { createApi } from "./api";
import { pollScoreChanges, PollHandle } from "./listener";
import { ScoreChangedEvent } from "./types";
import { Metrics } from "./metrics";

/**
 * Builds a signal handler that lets in-flight event processing finish and
 * closes the HTTP server and database cleanly before the process exits.
 */
export function createShutdownHandler(
  pollHandle: PollHandle,
  httpServer: Server,
  store: Store,
): (signal: string) => Promise<void> {
  let shuttingDown = false;

  return async (signal: string): Promise<void> => {
    if (shuttingDown) return;
    shuttingDown = true;
    console.log(`[main] Received ${signal}, shutting down gracefully...`);

    // Stop scheduling new polls and wait for any in-flight event processing to complete.
    await pollHandle.stop();

    // Stop accepting new HTTP connections and close existing ones.
    await new Promise<void>((resolve, reject) => {
      httpServer.close((err) => (err ? reject(err) : resolve()));
    });

    store.close();
    console.log("[main] Shutdown complete");
  };
}

async function main(): Promise<void> {
  const config = loadConfig();

  const store = new Store(config.db_path);
  const metrics = new Metrics();
  const notifier = new Notifier(config, store, metrics);

  // ── REST API ──────────────────────────────────────────────────────────
  const app = createApi(store, { metrics });
  const httpServer = app.listen(config.api_port, () => {
    console.log(`[api] Listening on port ${config.api_port}`);
  });

  // ── Event handler: score changed → notify investors ───────────────────
  const handleScoreChanged = async (
    event: ScoreChangedEvent,
  ): Promise<void> => {
    console.log(
      `[handler] ScoreChanged: project #${event.project_id} ` +
        `CQ:${event.old_credit_quality}→${event.new_credit_quality} ` +
        `GI:${event.old_green_impact}→${event.new_green_impact} ` +
        `rate:${event.old_rate_bps}→${event.new_rate_bps} bps`,
    );

    // Look up investors for this project
    const investors = store.getInvestorsForProject(event.project_id);
    if (investors.length === 0) {
      console.log(
        `[handler] No investors found for project #${event.project_id}`,
      );
      return;
    }

    await notifier.notifyInvestors(event, investors);
    metrics.recordEventProcessed();
  };

  // ── Start event polling ──────────────────────────────────────────────
  const pollHandle = await pollScoreChanges(
    config,
    handleScoreChanged,
    () => Promise.resolve(store.getLastProcessedLedger()),
    async (ledger) => store.markLedgerProcessed(ledger),
  );

  // ── Graceful shutdown on SIGTERM/SIGINT ────────────────────────────────
  const shutdown = createShutdownHandler(pollHandle, httpServer, store);
  const handleSignal = (signal: NodeJS.Signals): void => {
    shutdown(signal)
      .then(() => process.exit(0))
      .catch((err) => {
        console.error("[main] Error during shutdown:", err);
        process.exit(1);
      });
  };
  process.on("SIGTERM", () => handleSignal("SIGTERM"));
  process.on("SIGINT", () => handleSignal("SIGINT"));
}

// Only run when executed directly (`node dist/index.js`), not when imported for tests.
if (require.main === module) {
  main().catch((err) => {
    console.error("FATAL:", err);
    process.exit(1);
  });
}
