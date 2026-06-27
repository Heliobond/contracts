import express from "express";
import { Store } from "./db";
import { NotificationPreference } from "./types";

export function createApi(store: Store): express.Application {
  const app = express();
  app.use(express.json());

  // GET /health — health check
  app.get("/health", (_req, res) => {
    res.json({ status: "ok" });
  });

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

    if (email && typeof email !== "string") {
      res.status(400).json({ error: "email must be a string" });
      return;
    }

    if (webhook_url && typeof webhook_url !== "string") {
      res.status(400).json({ error: "webhook_url must be a string" });
      return;
    }

    const pref: NotificationPreference = {
      investor_address: address,
      email: email || undefined,
      webhook_url: webhook_url || undefined,
      enabled: enabled !== false,
      min_delta: typeof min_delta === "number" ? min_delta : 1,
      updated_at: new Date().toISOString(),
    };

    store.upsertPreference(pref);
    res.json(pref);
  });

  // DELETE /preferences/:address — remove an investor's preference
  app.delete("/preferences/:address", (req, res) => {
    store.deletePreference(req.params.address);
    res.status(204).send();
  });

  return app;
}
