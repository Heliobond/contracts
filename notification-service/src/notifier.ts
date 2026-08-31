import nodemailer from "nodemailer";
import { ScoreChangedEvent, WebhookPayload, ServiceConfig } from "./types";
import { Store } from "./db";
import { Metrics } from "./metrics";

/** Upper bound on tracked dedup keys, so long-running processes don't grow unbounded. */
const MAX_TRACKED_NOTIFICATIONS = 5000;

/** Upper bound on a single webhook delivery, so one unresponsive investor-supplied
 * endpoint can't stall the sequential notification loop for everyone behind it (#443). */
const WEBHOOK_TIMEOUT_MS = 10_000;

export class Notifier {
  private config: ServiceConfig;
  private store: Store;
  private metrics?: Metrics;
  private transporter?: nodemailer.Transporter;
  /**
   * Keys of (event, investor) pairs already successfully notified, to guard
   * against redelivery from the listener. Recorded per-investor and only
   * after a successful send, so a delivery failure for one investor doesn't
   * suppress a retry for them, and a redelivery doesn't re-notify investors
   * who already received it.
   */
  private notifiedRecipients: Set<string> = new Set();

  constructor(config: ServiceConfig, store: Store, metrics?: Metrics) {
    this.config = config;
    this.store = store;
    this.metrics = metrics;

    if (config.email_transport) {
      this.transporter = nodemailer.createTransport(config.email_transport);
      console.log("[notifier] Email transport configured");
    } else {
      console.log("[notifier] No email transport configured — email disabled");
    }
  }

  /** Dispatch notifications to all investors who hold shares in the project. */
  async notifyInvestors(
    event: ScoreChangedEvent,
    investorAddresses: string[],
  ): Promise<void> {
    const deltaCq = Math.abs(
      event.new_credit_quality - event.old_credit_quality,
    );
    const deltaGi = Math.abs(event.new_green_impact - event.old_green_impact);
    const maxDelta = Math.max(deltaCq, deltaGi);

    for (const addr of investorAddresses) {
      const pref = this.store.getPreference(addr);
      if (!pref || !pref.enabled) continue;
      if (maxDelta < pref.min_delta) continue;

      const hasEmail = !!(pref.email && this.transporter);
      const hasWebhook = !!pref.webhook_url;

      if (!hasEmail && !hasWebhook) continue;

      const recipientKey = this.recipientKey(event, addr);
      if (this.notifiedRecipients.has(recipientKey)) {
        console.log(
          `[notifier] Skipping duplicate notification to ${addr} for project #${event.project_id} at ledger ${event.ledger}`,
        );
        continue;
      }

      // Cross-restart dedup: check the persistent DB in case the in-memory
      // Set was cleared by a process restart (#335).
      if (
        this.store.hasBeenNotified(
          addr,
          event.project_id,
          event.ledger,
        )
      ) {
        this.rememberRecipient(recipientKey);
        continue;
      }

      const subject = `[Heliobond] Score change for project #${event.project_id}`;
      const text = this.formatEmailText(event, addr);

      try {
        if (hasEmail && this.transporter && pref.email) {
          await this.sendEmail(pref.email, subject, text);
          this.store.recordNotification(addr, event.project_id, "email", event.ledger);
          this.metrics?.recordNotificationSent();
        }
        if (hasWebhook && pref.webhook_url) {
          await this.sendWebhook(pref.webhook_url, event, addr);
          this.store.recordNotification(addr, event.project_id, "webhook", event.ledger);
          this.metrics?.recordNotificationSent();
        }
        this.rememberRecipient(recipientKey);
      } catch (err) {
        console.error(`[notifier] Failed to notify ${addr}:`, err);
      }
    }
  }

  private async sendEmail(
    to: string,
    subject: string,
    text: string,
  ): Promise<void> {
    if (!this.transporter) return;
    await this.transporter.sendMail({
      from: this.config.from_email,
      to,
      subject,
      text,
    });
    console.log(`[notifier] Email sent to ${to}`);
  }

  private async sendWebhook(
    url: string,
    event: ScoreChangedEvent,
    investorAddress: string,
  ): Promise<void> {
    const payload: WebhookPayload = {
      event: "score_changed",
      project_id: event.project_id,
      old_scores: {
        credit_quality: event.old_credit_quality,
        green_impact: event.old_green_impact,
      },
      new_scores: {
        credit_quality: event.new_credit_quality,
        green_impact: event.new_green_impact,
      },
      old_rate_bps: event.old_rate_bps,
      new_rate_bps: event.new_rate_bps,
      investor_address: investorAddress,
      timestamp: new Date(event.timestamp * 1000).toISOString(),
    };

    const response = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
      signal: AbortSignal.timeout(WEBHOOK_TIMEOUT_MS),
    });

    if (!response.ok) {
      throw new Error(
        `Webhook returned ${response.status}: ${await response.text()}`,
      );
    }
    console.log(`[notifier] Webhook sent to ${url} (${response.status})`);
  }

  /** Unique key identifying a specific on-chain ScoreChanged event delivered to one investor. */
  private recipientKey(
    event: ScoreChangedEvent,
    investorAddress: string,
  ): string {
    return [
      event.project_id,
      event.ledger,
      event.old_credit_quality,
      event.new_credit_quality,
      event.old_green_impact,
      event.new_green_impact,
      event.old_rate_bps,
      event.new_rate_bps,
      investorAddress,
    ].join(":");
  }

  private rememberRecipient(key: string): void {
    this.notifiedRecipients.add(key);
    if (this.notifiedRecipients.size > MAX_TRACKED_NOTIFICATIONS) {
      const oldest = this.notifiedRecipients.values().next().value;
      if (oldest !== undefined) this.notifiedRecipients.delete(oldest);
    }
  }

  private formatEmailText(
    event: ScoreChangedEvent,
    investorAddress: string,
  ): string {
    return [
      `Heliobond — Score Change Alert`,
      ``,
      `Project #${event.project_id} scores have been updated:`,
      ``,
      `  Credit Quality: ${event.old_credit_quality} → ${event.new_credit_quality}`,
      `  Green Impact:   ${event.old_green_impact} → ${event.new_green_impact}`,
      `  Interest Rate:  ${event.old_rate_bps} bps → ${event.new_rate_bps} bps`,
      ``,
      `Your address: ${investorAddress}`,
      `Ledger:       #${event.ledger}`,
      ``,
      `This affects your expected returns. Review your portfolio at`,
      `https://heliobond.io/portfolio`,
    ].join("\n");
  }
}
