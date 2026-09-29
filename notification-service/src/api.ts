import express, { Express, Request, Response, NextFunction } from "express";
import { Store } from "./db";
import { Metrics } from "./metrics";
import { NotificationPreference } from "./types";

export interface CreateApiOptions {
  metrics?: Metrics;
  rateLimit?: { windowMs: number; limit: number };
  allowedOrigins?: string[];
}

function createRateLimiter(windowMs: number, limit: number) {
  const hits = new Map<string, { count: number; resetAt: number }>();

  return (req: Request, res: Response, next: NextFunction): void => {
    const key = req.ip ?? "unknown";
    const now = Date.now();
    const record = hits.get(key);

    if (!record || now > record.resetAt) {
      hits.set(key, { count: 1, resetAt: now + windowMs });
      return next();
    }

    if (record.count >= limit) {
      res.set("Retry-After", String(Math.ceil((record.resetAt - now) / 1000)));
      res.status(429).json({ error: "Too Many Requests" });
      return;
    }

    record.count++;
    next();
  };
}

function createCorsMiddleware(allowedOrigins: string[]) {
  return (req: Request, res: Response, next: NextFunction): void => {
    const origin = req.headers.origin;
    if (origin && allowedOrigins.includes(origin)) {
      res.set("Access-Control-Allow-Origin", origin);
      res.set("Vary", "Origin");
      res.set("Access-Control-Allow-Methods", "GET, PUT, DELETE, OPTIONS");
      res.set("Access-Control-Allow-Headers", "Content-Type");
    }

    if (req.method === "OPTIONS") {
      res.status(204).end();
      return;
    }

    next();
  };
}

function validatePreferenceBody(body: unknown): body is Partial<NotificationPreference> & { min_delta?: number } {
  if (!body || typeof body !== "object" || Array.isArray(body)) {
    return false;
  }
  const b = body as Record<string, unknown>;
  if (b.email !== undefined && typeof b.email !== "string") return false;
  if (b.webhook_url !== undefined && typeof b.webhook_url !== "string") return false;
  if (b.enabled !== undefined && typeof b.enabled !== "boolean") return false;
  if (b.min_delta !== undefined && (typeof b.min_delta !== "number" || b.min_delta < 0)) return false;
  if (b.webhook_url !== undefined && b.webhook_url !== "") {
    try {
      new URL(b.webhook_url);
    } catch {
      return false;
    }
  }
  return true;
}

export function createApi(store: Store, options: CreateApiOptions = {}): Express {
  const { metrics, rateLimit, allowedOrigins } = options;

  const app = express();
  app.use(express.json());

  if (allowedOrigins && allowedOrigins.length > 0) {
    app.use(createCorsMiddleware(allowedOrigins));
  }

  if (rateLimit) {
    app.use(createRateLimiter(rateLimit.windowMs, rateLimit.limit));
  }

  // Health check with DB connectivity
  app.get("/health", (req: Request, res: Response) => {
    const dbHealthy = store.isHealthy();
    if (dbHealthy) {
      res.json({
        status: "ok",
        database: "connected",
        uptime: process.uptime(),
      });
    } else {
      res.status(503).json({
        status: "degraded",
        database: "disconnected",
        uptime: process.uptime(),
      });
    }
  });

  // Metrics endpoint
  app.get("/metrics", (req: Request, res: Response) => {
    if (!metrics) {
      res.status(404).json({ error: "Metrics not available" });
      return;
    }
    res.json(metrics.snapshot());
  });

  // PUT /preferences/:address - upsert preference (merge with existing)
  app.put("/preferences/:address", async (req: Request, res: Response) => {
    const { address } = req.params;

    if (!validatePreferenceBody(req.body)) {
      const b = req.body as Record<string, unknown> | null;
      if (!b || typeof b !== "object" || Array.isArray(b)) {
        return res.status(400).json({ error: "Body must be a JSON object" });
      }
      if (b.email !== undefined && typeof b.email !== "string") {
        return res.status(400).json({ error: "email must be a string" });
      }
      if (b.webhook_url !== undefined && typeof b.webhook_url !== "string") {
        return res.status(400).json({ error: "webhook_url must be a string" });
      }
      if (b.webhook_url !== undefined && b.webhook_url !== "") {
        try {
          new URL(b.webhook_url);
        } catch {
          return res.status(400).json({ error: "webhook_url must be a valid URL" });
        }
      }
      if (b.enabled !== undefined && typeof b.enabled !== "boolean") {
        return res.status(400).json({ error: "enabled must be a boolean" });
      }
      if (b.min_delta !== undefined && (typeof b.min_delta !== "number" || b.min_delta < 0)) {
        return res.status(400).json({ error: "min_delta must be a non-negative number" });
      }
      return res.status(400).json({ error: "Invalid request body" });
    }

    const existing = store.getPreference(address);

    const body = req.body as Partial<NotificationPreference> & { min_delta?: number };

    // Merge with existing preference, preserving fields not provided in the request
    const mergedPreference: NotificationPreference = {
      investor_address: address,
      email: body.email ?? existing?.email,
      webhook_url: body.webhook_url ?? existing?.webhook_url,
      enabled: body.enabled ?? existing?.enabled ?? true,
      min_delta: body.min_delta ?? existing?.min_delta ?? 1,
      updated_at: new Date().toISOString(),
    };

    // Enforce at least one notification channel
    if (!mergedPreference.email && !mergedPreference.webhook_url) {
      return res
        .status(400)
        .json({ error: "At least one of email or webhook_url must be provided" });
    }

    store.upsertPreference(mergedPreference);
    return res.json(mergedPreference);
  });

  // GET /preferences/:address - get single preference
  app.get("/preferences/:address", (req: Request, res: Response) => {
    const { address } = req.params;
    const pref = store.getPreference(address);
    if (!pref) {
      return res.status(404).json({ error: "Preference not found" });
    }
    res.json(pref);
  });

  // DELETE /preferences/:address - delete preference
  app.delete("/preferences/:address", (req: Request, res: Response) => {
    const { address } = req.params;
    store.deletePreference(address);
    res.status(204).end();
  });

  // GET /preferences - list all preferences
  app.get("/preferences", (req: Request, res: Response) => {
    const prefs = store.listPreferences();
    res.json(prefs);
  });

  // GET /notifications/history - paginated notification history
  app.get("/notifications/history", (req: Request, res: Response) => {
    const limit = Math.min(Math.max(parseInt(req.query.limit as string) || 50, 1), 200);
    const offset = Math.max(parseInt(req.query.offset as string) || 0, 0);
    const investor_address = req.query.investor_address as string | undefined;

    const page = store.listNotificationHistory({ investor_address, limit, offset });
    res.json(page);
  });

  return app;
}