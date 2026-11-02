/**
 * Enterprise Telemetry & Notification Dispatcher
 * Dispatches production webhooks to Discord/Slack and sends verified receipts via Resend.
 */

export interface EventPayload {
  eventName: string;
  grantId: string;
  txSignature: string;
  timestamp: number;
  details: Record<string, any>;
}

export class NotificationDispatcher {
  private resendApiKey: string | undefined;
  private discordWebhookUrl: string | undefined;
  private alertEmailRecipient: string;

  constructor() {
    this.resendApiKey = process.env.RESEND_API_KEY;
    this.discordWebhookUrl = process.env.DISCORD_WEBHOOK_URL;
    this.alertEmailRecipient = process.env.ALERT_EMAIL || "organizers@univibeathon.edu";
  }

  public async dispatch(event: EventPayload): Promise<void> {
    console.log(`[DISPATCHER] Processing event: ${event.eventName} for Grant #${event.grantId}`);

    const tasks: Promise<void>[] = [];

    if (this.discordWebhookUrl) {
      tasks.push(this.sendDiscordWebhook(event));
    } else {
      console.warn("[DISPATCHER] DISCORD_WEBHOOK_URL not set; skipping webhook.");
    }

    if (this.resendApiKey) {
      tasks.push(this.sendResendEmail(event));
    } else {
      console.warn("[DISPATCHER] RESEND_API_KEY not configured; skipping email dispatch.");
    }

    const results = await Promise.allSettled(tasks);
    results.forEach((r, idx) => {
      if (r.status === "rejected") {
        console.error(`[DISPATCHER] Sub-task ${idx} failed:`, r.reason);
      }
    });
  }

  private async sendDiscordWebhook(event: EventPayload): Promise<void> {
    const embed = {
      title: `UniVibe Protocol Alert: ${event.eventName}`,
      color: event.eventName.includes("Approved") ? 0x00ff88 : 0x7c3aed,
      fields: [
        { name: "Grant ID", value: event.grantId, inline: true },
        { name: "Time", value: new Date(event.timestamp * 1000).toISOString(), inline: true },
        { name: "Explorer TX", value: `[View Solana TX](https://explorer.solana.com/tx/${event.txSignature}?cluster=devnet)` },
        { name: "Metadata", value: "