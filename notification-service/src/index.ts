import { loadConfig } from "./config";
import { Store } from "./db";
import { Notifier } from "./notifier";
import { createApi } from "./api";
import { pollScoreChanges } from "./listener";
import { ScoreChangedEvent } from "./types";

async function main(): Promise<void> {
  const config = loadConfig();

  if (!config.registry_contract_id) {
    console.error(
      "FATAL: REGISTRY_CONTRACT_ID environment variable is required",
    );
    process.exit(1);
  }

  const store = new Store(config.db_path);
  const notifier = new Notifier(config, store);

  // ── REST API ──────────────────────────────────────────────────────────
  const app = createApi(store);
  app.listen(config.api_port, () => {
    console.log(`[api] Listening on port ${config.api_port}`);
  });

  // ── Event handler: score changed → notify investors ───────────────────
  const handleScoreChanged = async (event: ScoreChangedEvent): Promise<void> => {
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
  };

  // ── Start event polling ──────────────────────────────────────────────
  await pollScoreChanges(
    config,
    handleScoreChanged,
    () => Promise.resolve(store.getLastProcessedLedger()),
    async (ledger) => store.markLedgerProcessed(ledger),
  );
}

main().catch((err) => {
  console.error("FATAL:", err);
  process.exit(1);
});
