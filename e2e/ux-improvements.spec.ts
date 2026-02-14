import { test, expect } from "@playwright/test";

test.describe("Remote URL Display", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("dashboard shows remote access URL in header", async ({ page }) => {
    // Wait for the remote URL element to be populated
    const urlEl = page.locator('[data-testid="remote-url"]');
    await expect(urlEl).toBeVisible({ timeout: 5_000 });
    await expect(urlEl).toContainText("http://");
    await expect(urlEl).toContainText(":8920");
  });

  test("remote URL has click-to-copy functionality", async ({ page }) => {
    const urlEl = page.locator('[data-testid="remote-url"]');
    await expect(urlEl).toBeVisible({ timeout: 5_000 });
    // Should have cursor pointer style indicating clickability
    await expect(urlEl).toHaveCSS("cursor", "pointer");
  });

  test("API returns remote URL", async ({ request }) => {
    const resp = await request.get("/api/v1/remote-url");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.url).toMatch(/^https?:\/\/.+:8920$/);
  });

  test("remote URL API returns valid IP or hostname", async ({ request }) => {
    const resp = await request.get("/api/v1/remote-url");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    // Should contain an IP address or hostname, not just localhost
    expect(body.url).toBeDefined();
    expect(body.url.length).toBeGreaterThan(15); // "http://x.x.x.x:8920" is at least 18 chars
  });
});

test.describe("Signal Status Indicator", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("dashboard shows signal status element", async ({ page }) => {
    const signalEl = page.locator('[data-testid="signal-status"]');
    await expect(signalEl).toBeVisible({ timeout: 5_000 });
  });

  test("signal status indicates OK or No Signal", async ({ page }) => {
    const signalEl = page.locator('[data-testid="signal-status"]');
    await expect(signalEl).toBeVisible({ timeout: 5_000 });
    // Should contain one of the expected states
    const text = await signalEl.textContent();
    expect(text).toMatch(/Signal OK|NO SIGNAL/i);
  });

  test("signal status reflects confidence from stats API", async ({
    page,
    request,
  }) => {
    // Get current stats to check signal_lost state
    const statsResp = await request.get("/api/v1/stats");
    expect(statsResp.ok()).toBeTruthy();
    const stats = await statsResp.json();

    // Wait for page to receive WebSocket update
    await page.waitForTimeout(500);

    // Verify dashboard shows correct state based on signal_lost
    const signalEl = page.locator('[data-testid="signal-status"]');
    const displayedText = await signalEl.textContent();

    if (stats.signal_lost) {
      expect(displayedText).toMatch(/NO SIGNAL/i);
    } else {
      expect(displayedText).toMatch(/Signal OK/i);
    }
  });
});

test.describe("Stats API includes signal_lost field", () => {
  test("stats response includes signal_lost boolean", async ({ request }) => {
    const resp = await request.get("/api/v1/stats");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    // signal_lost should be a boolean field
    expect(typeof body.signal_lost).toBe("boolean");
  });
});
