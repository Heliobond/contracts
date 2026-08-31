import express, { Request, Response, NextFunction } from "express";
import rateLimit, { Options as RateLimitOptions } from "express-rate-limit";
import { Store } from "./db";
import { NotificationPreference } from "./types";
import { Metrics } from "./metrics";

export interface ApiOptions {
  /** Overrides for the public rate limiter (mainly for tests). */
  rateLimit?: Partial<RateLimitOptions>;
  /** If set, only requests whose Origin header is in this list will receive CORS headers. */
  allowedOrigins?: string[];
  /** Optional metrics instance to expose via GET /metrics. */
  metrics?: Metrics;
}

const DEFAULT_HISTORY_LIMIT = 50;
const MAX_HISTORY_LIMIT = 200;

/** Clamp a query-string pagination param to a safe positive integer, falling back to `fallback`. */
function parsePaginationParam("
  value: unknown,
  fallback: number,
  max?: number,
): number {
  const parsed = typeof value === "string" ? parseInt(value, 10) : NaN;
  if (!Number.isFinite(parsed) || parsed < 0) return fallback;
  return max !== undefined ? Math.min(parsed, max) : parsed;
}

export function createApi(
  store: Store,
  options: ApiOptions = {},
): express.Application {
  const app = express();
  app.use(express.json());

  // ┌ CORS │
  // When allowedOrigins is configured, only matching Origin values receive
  // the Access-Control-Allow-Origin header; non-matching origiins get no
  // CORS headers, so the browser blocks the cross-origin read. When the
  // list is omitted, no CORS headers are added (framework default).
  app.use((req: Request, res: Response, next: NextFunction) => {
    const origin = req.headers.origin;
    if (origin && options.allowedOrigins?.includes(origin)) {
      res.setHeader("Access-Control-Allow-Origin", origin);
      res.setHeader("Access-Control-Allow-Methods", "GET, PUT, DELETE, OPTIONS");
      res.setHeader("Access-Control-Allow-Headers", "Content-Type");
    }
    if (req.method === "OPTIONS") {
      res.status(204).send();
      return;
    }
    next();
  });

  // Basic rate limiting to prevent abuse of these publicly exposed routes.
  app.use(
    rateLimit({
      windowMs: 15 * 60 * 1000,
      limit: 100,
      standardHeaders: true,
      legacyHeaders: false,
      ...options.rateLimit,
    }),
  );

  // GET /health — health check with DB connectivity
  app.get("/health", (_req, res) => {
    try {
      const dbOk = store.isHealthy();
      const status = dbOk ? "ok" : "degraded";
      const code = dbOk ? 200 : 503;
      res.status(code).json({
        status,
        database: dbOk ? "connected" : "disconnected",
        uptime: process.uptime(),
      });
    } catch {
      res.status(503).json({
        status: "degraded",
        database: "error",
        uptime: process.uptime(),
      });
    }
  });

  // GET /metrics — expose in-memory counters (events processed, notifications sent)
  if (options.metrics) {
    app.get("/metrics", (_req, res) => {
      res.json(options.metrics!.snapshot());
    });
  }

  // GET /preferences — list all notification preferences
  app.get("/preferences", (_req, res) => {
    const prefs = store.listPreferences();
    res.json(prefs);
  });

  // GET /preferences/:address — get preference for a specific investor
  app.get("/preferences/:address", (req, res) => {
    const pref = store.getPreference(req.params.address);
    if (!pref) {
      res.status(404).json({ error: "preference not found" });
      return;
    }
    res.json(pref);
  });

  // PUT /preferences/:address — create or update an investor's preference
  app.put("/preferences/:address", (req, res) => {
    const { email, webhook_url, enabled, min_delta } = req.body;
    const address = req.params.address;

    if (!address) {
      res.status(400).json({ error: "address is required" });
      return;
    }

    // Reject requests with no body or a non-object body
    if (!req.body || typeof req.body !== "object" || Array.isArray(req.body)) {
      res.status(400).json({ error: "request body must be a JSON object" });
      return;
    }

    if (email !== undefined && typeof email !== "string") {
      res.status(400).json({ error: "email must be a string" });
      return;
    }

    if (webhook_url !== undefined && typeof webhook_url !== "string") {
      res.status(400).json({ error: "webhook_url must be a string" });
      return;
    }

    if (webhook_url !== undefined && typeof webhook_url === "string" && webhook_url.length > 0) {
      try {
        new URL(webhook_url);
      } catch {
        res.status(400).json({ error: "webhook_url must be a valid URL" });
        return;
      }
    }

    if (enabled !== undefined && typeof enabled !== "boolean") {
      res.status(400).json({ error: "enabled must be a boolean" });
      return;
    }

    if (min_delta !== undefined && (typeof min_delta !== "number" || !Number.isFinite(min_delta) || min_delta < 0)) {
      res.status(400).json({ error: "min_delta must be a non-negative number" });
      return;
    }

    // Fetch existing preference to merge with incoming fields, so that
    // omitting a field in the request preserves the existing value (#375).
    const existing = store.getPreference(address);

    const pref: NotificationPreference = {
      investor_address: address,
      email: email !== undefined ? (email || undefined) : existing?.email,
      webhook_url: webhook_url !== undefined ? (webhook_url || undefined) : existing?.webhook_url,
      enabled: enabled !== undefined ? enabled : (existing?.enabled ?? true),
      min_delta: typeof min_delta === "number" ? min_delta : (existing?.min_delta ?? 1),
      updated_at: new Date().toISOString(),
    };

    if (!pref.email && !pref.webhook_url) {
      res.status(400).json({ error: "at least one of email or webhook_url must be provided" });
      return;
    }

    store.upsertPreference(pref);
    res.json(pref);
  });

  // DELETE /preferences/:address — remove an investor's preference
  app.delete("/preferences/:address", (req, res) => {
    store.deletePreference(req.params.address);
    res.status(204).send();
  });

  // GET /notifications/history — paginated notification history, optionally
  // filtered to a single investor via ?investor_address=
  app.get("/notifications/history", (req, res) => {
    const limit = parsePaginationParam(
      req.query.limit,
      DEFAULT_HISTORY_LIMIT,
      MAX_HISTORY_LIMIT,
    );
    const offset = parsePaginationParam(req.query.offset, 0);
    const investor_address =
      typeof req.query.investor_address === "string"
        ? req.query.investor_address
        : undefined;

    const page = store.listNotificationHistory({
      investor_address,
      limit,
      offset,
    });
    res.json(page);
  });

  return app;
}
