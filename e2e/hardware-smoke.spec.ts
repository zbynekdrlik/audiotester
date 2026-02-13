import { test, expect } from "@playwright/test";

// These tests only run when AUDIOTESTER_HARDWARE_TEST=true
// They require real ASIO hardware (VASIO 8 loopback on iem.lan)
const isHardwareTest = process.env.AUDIOTESTER_HARDWARE_TEST === "true";

test.describe("Hardware Smoke Tests", () => {
  test.skip(!isHardwareTest, "Requires AUDIOTESTER_HARDWARE_TEST=true");

  test("device is auto-selected", async ({ request }) => {
    const resp = await request.get("/api/v1/status");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.device).toBeTruthy();
    expect(body.device).toContain("VASIO");
  });

  test("monitoring is running", async ({ request }) => {
    const resp = await request.get("/api/v1/status");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.monitoring).toBe(true);
    expect(body.state).toBe("Running");
  });

  test("latency is in valid loopback range", async ({ request }) => {
    const resp = await request.get("/api/v1/stats");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.current_latency).toBeGreaterThan(0);
    expect(body.current_latency).toBeLessThan(50);
  });

  test("measurement count increases over time", async ({ request }) => {
    const resp1 = await request.get("/api/v1/stats");
    const stats1 = await resp1.json();
    const count1 = stats1.measurement_count;

    // Wait for more measurements
    await new Promise((r) => setTimeout(r, 3000));

    const resp2 = await request.get("/api/v1/stats");
    const stats2 = await resp2.json();
    const count2 = stats2.measurement_count;

    expect(count2).toBeGreaterThan(count1);
  });

  test("WebSocket pushes real data to dashboard", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByText("Connected")).toBeVisible({ timeout: 10_000 });

    // Wait for real latency data to appear
    const latencyEl = page.locator('[data-testid="latency-value"]');
    await expect(latencyEl).toBeVisible();

    // Should show a real numeric value, not "--" or "0.00"
    await page.waitForTimeout(5000);
    const text = await latencyEl.textContent();
    expect(text).toBeTruthy();
    expect(text).not.toBe("--");
    const value = parseFloat(text!.replace(/[^\d.]/g, ""));
    expect(value).toBeGreaterThan(0);
    expect(value).toBeLessThan(50);
  });

  test("charts are rendered", async ({ page }) => {
    await page.goto("/");
    await page.waitForTimeout(3000);

    // Check that canvas elements exist for charts
    const canvases = page.locator("canvas");
    const count = await canvases.count();
    expect(count).toBeGreaterThanOrEqual(1);
  });

  test("no or minimal sample loss on loopback", async ({ request }) => {
    const resp = await request.get("/api/v1/stats");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    // On a clean loopback, there should be zero or very few lost samples
    expect(body.total_lost).toBeLessThan(10);
  });
});
