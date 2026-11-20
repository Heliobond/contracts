/** Simple in-memory counters for observability. Thread-safe for single-process use. */
export class Metrics {
  private eventsProcessed = 0;
  private notificationsSent = 0;

  /** Record that one on-chain event was decoded and forwarded for processing. */
  recordEventProcessed(): void {
    this.eventsProcessed++;
  }

  /** Record that one notification was successfully delivered to a recipient. */
  recordNotificationSent(): void {
    this.notificationsSent++;
  }

  /** Return a snapshot of all counters. */
  snapshot(): { eventsProcessed: number; notificationsSent: number } {
    return {
      eventsProcessed: this.eventsProcessed,
      notificationsSent: this.notificationsSent,
    };
  }
}
