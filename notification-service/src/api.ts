import express from 'express';
import { NotificationPreference } from '.types';
import { getPreference, savePreference } from './storage';

const router = express.Router();

router.put('/preferences/:address', async (req, res) => {
  const { address } = req.params;
  const { email, webhook_url, enabled } = req.body || {};

  // Validate types
  if (email !== undefined && typeof email !== 'string') {
    return res.status(400).json({ error: 'email must be a string' });
  }
  if (webhook_url !== undefined && typeof webhook_url !== 'string') {
    return res.status(400).json({ error: 'webhook_url must be a string' });
  }
  if (enabled !== undefined && typeof enabled !== 'boolean') {
    return res.status(400).json({ error: 'enabled must be a boolean' });
  }

  const existing = await getPreference(address);

  // Compute resulting preference
  const resultingEmail = email !== undefined ? email : existing?.email ?? '';
  const resultingWebhook = webhook_url !== undefined ? webhook_url : existing?.webhook_url ?? '';

  // Enforce at least one notification channel
  if (!resultingEmail && !resultingWebhook) {
    return res.status(400).json({ error: 'At least one of email or webhook_url must be provided' });
  }

  const preference: NotificationPreference = {
    email: resultingEmail,
    webhook_url: resultingWebhook,
    enabled: enabled !== undefined ? enabled : existing?.enabled ?? true,
  };

  await savePreference(address, preference);

  return res.json(preference);
});

export default router;