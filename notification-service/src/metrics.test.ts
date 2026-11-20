import { describe, it, expect } from "vitest";
import { Metrics } from "./metrics";

describe("Metrics", () => {
  it("starts with zero counters", () => {
    const m = new Metrics();
    expect(m.snapshot()).toEqual({
      eventsProcessed: 0,
      notificationsSent: 0,
    });
  });

  it("increments eventsProcessed", () => {
    const m = new Metrics();
    m.recordEventProcessed();
    m.recordEventProcessed();
    expect(m.snapshot().eventsProcessed).toBe(2);
  });

  it("increments notificationsSent", () => {
    const m = new Metrics();
    m.recordNotificationSent();
    m.recordNotificationSent();
    m.recordNotificationSent();
    expect(m.snapshot().notificationsSent).toBe(3);
  });

  it("tracks both counters independently", () => {
    const m = new Metrics();
    m.recordEventProcessed();
    m.recordNotificationSent();
    m.recordNotificationSent();
    expect(m.snapshot()).toEqual({
      eventsProcessed: 1,
      notificationsSent: 2,
    });
  });
});
