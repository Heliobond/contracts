import { describe, it, expect, afterEach } from "vitest";
import request from "supertest";
import { createApi } from "./api";
import { Store } from "./db";
import { Metrics } from "./metrics";

function makeStore(): Store {
  return new Store(":memory:");
}

describe("createApi rate limiting", () => {
  let store: Store;

  afterEach(() => {
    store?.close();
  });

  it("allows requests under the limit", async () => {
    store = makeStore();
    const app = createApi(store, { rateLimit: { windowMs: 60_000, limit: 3 } });

    const res = await request(app).get("/health");
    expect(res.status).toBe(200);
  });

  it("returns 429 once the public endpoint limit is exceeded", async () => {
    store = makeStore();
    const app = createApi(store, { rateLimit: { windowMs: 60_000, limit: 3 } });

    for (let i = 0; i < 3; i++) {
      const res = await request(app).get("/health");
      expect(res.status).toBe(200);
    }

    const limited = await request(app).get("/health");
    expect(limited.status).toBe(429);
  });

  it("applies the limit across different public routes, not per-route", async () => {
    store = makeStore();
    const app = createApi(store, { rateLimit: { windowMs: 60_000, limit: 2 } });

    await request(app).get("/health");
    await request(app).get("/preferences");
    const limited = await request(app).get("/health");

    expect(limited.status).toBe(429);
  });
});

describe("GET /notifications/history", () => {
  let store: Store;

  afterEach(() => {
    store?.close();
  });

  it("returns a bounded page instead of the full unbounded history", async () => {
    store = makeStore();
    for (let i = 0; i < 5; i++) {
      store.recordNotification("GINVESTOR", 1, "webhook", 100 + i);
    }
    const app = createApi(store);

    const res = await request(app)
      .get("/notifications/history")
      .query({ limit: 2, offset: 0 });

    expect(res.status).toBe(200);
    expect(res.body.items).toHaveLength(2);
    expect(res.body.total).toBe(5);
    expect(res.body.limit).toBe(2);
    expect(res.body.offset).toBe(0);
    // Most recent (highest ledger) first.
    expect(res.body.items[0].ledger).toBe(104);
    expect(res.body.items[1].ledger).toBe(103);
  });

  it("filters by investor_address when provided", async () => {
    store = makeStore();
    store.recordNotification("GINVESTOR", 1, "webhook", 100);
    store.recordNotification("GOTHER", 2, "email", 101);
    const app = createApi(store);

    const res = await request(app)
      .get("/notifications/history")
      .query({ investor_address: "GOTHER" });

    expect(res.status).toBe(200);
    expect(res.body.items).toHaveLength(1);
    expect(res.body.items[0].investor_address).toBe("GOTHER");
    expect(res.body.total).toBe(1);
  });

  it("clamps an excessive limit instead of returning everything", async () => {
    store = makeStore();
    for (let i = 0; i < 3; i++) {
      store.recordNotification("GINVESTOR", 1, "webhook", 100 + i);
    }
    const app = createApi(store);

    const res = await request(app)
      .get("/notifications/history")
      .query({ limit: 999999 });

    expect(res.status).toBe(200);
    expect(res.body.limit).toBeLessThanOrEqual(200);
  });

  it("defaults to an empty page when no notifications have been sent", async () => {
    store = makeStore();
    const app = createApi(store);

    const res = await request(app).get("/notifications/history");

    expect(res.status).toBe(200);
    expect(res.body.items).toEqual([]);
    expect(res.body.total).toBe(0);
  });
});

// ── Issue #218: input validation for malformed payloads ─────────────────────

describe("CORS configuration", () => {
  let store: Store;

  afterEach(() => {
    store?.close();
  });

  it("returns 400 when email is not a string", async () => {
    store = makeStore();
    const app = createApi(store);

    const res = await request(app)
      .put("/preferences/GINVESTOR")
      .send({ email: 123 });

    expect(res.status).toBe(400);
    expect(res.body.error).toMatch(/email/);
  });

  it("returns 400 when webhook_url is not a string", async () => {
    store = makeStore();
    const app = createApi(store);

    const res = await request(app)
      .put("/preferences/GINVESTOR")
      .send({ webhook_url: true });

    expect(res.status).toBe(400);
    expect(res.body.error).toMatch(/webhook_url/);
  });

  it("returns 400 when webhook_url is not a valid URL", async () => {
    store = makeStore();
    const app = createApi(store);

    const res = await request(app)
      .put("/preferences/GINVESTOR")
      .send({ webhook_url: "not-a-url" });

    expect(res.status).toBe(400);
    expect(res.body.error).toMatch(/valid URL/);
  });

  it("returns 400 when enabled is not a boolean", async () => {
    store = makeStore();
    const app = createApi(store);

    const res = await request(app)
      .put("/preferences/GINVESTOR")
      .send({ enabled: "yes" });

    expect(res.status).toBe(400);
    expect(res.body.error).toMatch(/enabled/);
  });

  it("returns 400 when min_delta is negative", async () => {
    store = makeStore();
    const app = createApi(store);

    const res = await request(app)
      .put("/preferences/GINVESTOR")
      .send({ min_delta: -5 });

    expect(res.status).toBe(400);
    expect(res.body.error).toMatch(/min_delta/);
  });

  it("returns 400 when body is an array", async () => {
    store = makeStore();
    const app = createApi(store);

    const res = await request(app)
      .put("/preferences/GINVESTOR")
      .send([{ email: "test@example.com" }]);

    expect(res.status).toBe(400);
    expect(res.body.error).toMatch(/JSON object/);
  });

  it("sets Access-Control-Allow-Origin for a matching origin", async () => {
    store = makeStore();
    const app = createApi(store, {
      allowedOrigins: ["https://heliobond.io"],
    });

    const res = await request(app)
      .get("/health")
      .set("Origin", "https://heliobond.io");

    expect(res.headers["access-control-allow-origin"]).toBe(
      "https://heliobond.io",
    );
  });

  it("does not set CORS headers for an origin not in the allow list", async () => {
    store = makeStore();
    const app = createApi(store, {
      allowedOrigins: ["https://heliobond.io"],
    });

    const res = await request(app)
      .get("/health")
      .set("Origin", "https://evil.example.com");

    expect(res.headers["access-control-allow-origin"]).toBeUndefined();
  });

  it("responds 204 to OPTIONS preflight requests from an allowed origin", async () => {
    store = makeStore();
    const app = createApi(store, {
      allowedOrigins: ["https://heliobond.io"],
    });

    const res = await request(app)
      .options("/preferences")
      .set("Origin", "https://heliobond.io");

    expect(res.status).toBe(204);
    expect(res.headers["access-control-allow-methods"]).toContain("GET");
  });

  it("does not add CORS headers when allowedOrigins is not configured", async () => {
    store = makeStore();
    const app = createApi(store);

    const res = await request(app)
      .get("/health")
      .set("Origin", "https://heliobond.io");

    expect(res.headers["access-control-allow-origin"]).toBeUndefined();
  });

  it("accepts a valid payload and returns 200", async () => {
    store = makeStore();
    const app = createApi(store);

    const res = await request(app).put("/preferences/GINVESTOR").send({
      email: "test@example.com",
      webhook_url: "https://example.com/webhook",
      enabled: true,
      min_delta: 5,
    });

    expect(res.status).toBe(200);
    expect(res.body.email).toBe("test@example.com");
    expect(res.body.webhook_url).toBe("https://example.com/webhook");
    expect(res.body.enabled).toBe(true);
    expect(res.body.min_delta).toBe(5);
  });
});

// ── Issue #219: health-check with DB connectivity ──────────────────────────

describe("GET /metrics", () => {
  let store: Store;

  afterEach(() => {
    store?.close();
  });

  it("returns status ok and database connected when DB is healthy", async () => {
    store = makeStore();
    const app = createApi(store);

    const res = await request(app).get("/health");

    expect(res.status).toBe(200);
    expect(res.body.status).toBe("ok");
    expect(res.body.database).toBe("connected");
    expect(typeof res.body.uptime).toBe("number");
  });

  it("returns 503 when the database is disconnected", async () => {
    store = makeStore();
    const app = createApi(store);

    // Close the DB to simulate disconnection
    store.close();

    const res = await request(app).get("/health");

    expect(res.status).toBe(503);
    expect(res.body.status).toBe("degraded");
  });
  it("returns snapshot from the provided Metrics instance", async () => {
    store = makeStore();
    const metrics = new Metrics();
    metrics.recordEventProcessed();
    metrics.recordEventProcessed();
    metrics.recordNotificationSent();
    const app = createApi(store, { metrics });

    const res = await request(app).get("/metrics");

    expect(res.status).toBe(200);
    expect(res.body).toEqual({
      eventsProcessed: 2,
      notificationsSent: 1,
    });
  });

  it("returns 404 when no Metrics instance is provided", async () => {
    store = makeStore();
    const app = createApi(store);

    const res = await request(app).get("/metrics");
    expect(res.status).toBe(404);
  });
});
