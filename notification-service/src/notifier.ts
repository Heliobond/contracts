import nodemailer from "nodemailer";
import {
  ScoreChangedEvent,
  NotificationPreference,
  WebhookPayload,
  ServiceConfig,
} from "./types";
import { Store } from "./db";

export class Notifier {
  private config: ServiceConfig;
  private store: Store;
  private transporter?: nodemailer.Transporter;

  constructor(config: ServiceConfig, store: Store) {
    this.config = config;
    this.store = store;

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
    const deltaGi = Math.abs(
      event.new_green_impact - event.old_green_impact,
    );
    const maxDelta = Math.max(deltaCq, deltaGi);

    for (const addr of investorAddresses) {
      const pref = this.store.getPreference(addr);
      if (!pref || !pref.enabled) continue;
      if (maxDelta < pref.min_delta) continue;

      const hasEmail = !!(pref.email && this.transporter);
      const hasWebhook = !!pref.webhook_url;

      if (!hasEmail && !hasWebhook) continue;

      const subject = `[Heliobond] Score change for project #${event.project_id}`;
      const text = this.formatEmailText(event, addr);

      try {
        if (hasEmail && this.transporter && pref.email) {
          await this.sendEmail(pref.email, subject, text);
        }
        if (hasWebhook && pref.webhook_url) {
          await this.sendWebhook(pref.webhook_url, event, addr);
        }
      } catch (err) {
        console.error(
          `[notifier] Failed to notify ${addr}:`,
          err,
        );
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
    });

    if (!response.ok) {
      throw new Error(
        `Webhook returned ${response.status}: ${await response.text()}`,
      );
    }
    console.log(`[notifier] Webhook sent to ${url} (${response.status})`);
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
