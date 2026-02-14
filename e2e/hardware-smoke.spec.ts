import { test, expect } from "@playwright/test";
import {
  disconnectVasio8Loopback,
  reconnectVasio8Loopback,
  isVbanTextAvailable,
  queryProperty,
} from "./helpers/vban-text";

// These tests only run when AUDIOTESTER_HARDWARE_TEST=true
// They require real ASIO hardware (VASIO 8 loopback on iem.lan)
const isHardwareTest = process.env.AUDIOTESTER_HARDWARE_TEST === "true";
const vbmatrixHost = process.env.AUDIOTESTER_HOST || "iem.lan";

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

  test("signal status shows OK when loopback is connected", async ({
    page,
    request,
  }) => {
    // On hardware with proper loopback, signal_lost should be false
    const resp = await request.get("/api/v1/stats");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();

    // With valid loopback, signal should be detected
    expect(body.signal_lost).toBe(false);

    // Dashboard should also show "Signal OK"
    await page.goto("/");
    const signalEl = page.locator('[data-testid="signal-status"]');
    await expect(signalEl).toBeVisible({ timeout: 5_000 });
    await expect(signalEl).toHaveText("Signal OK");
  });

  test("signal detection has sufficient confidence", async ({ request }) => {
    // This test verifies that confidence threshold is correctly implemented
    // A valid loopback should have high confidence
    const resp = await request.get("/api/v1/stats");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();

    // If we have measurements, signal_lost should be false with good loopback
    if (body.measurement_count > 0) {
      expect(body.signal_lost).toBe(false);
    }
  });

  test("confidence value is reported in stats", async ({ request }) => {
    const resp = await request.get("/api/v1/stats");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(typeof body.confidence).toBe("number");
    // With active loopback, confidence should be high
    expect(body.confidence).toBeGreaterThan(0.3);
  });
});

test.describe("VBMatrix Loopback Signal Detection", () => {
  test.skip(!isHardwareTest, "Requires AUDIOTESTER_HARDWARE_TEST=true");

  // Ensure loopback is restored after each test
  test.afterEach(async ({ request }) => {
    await reconnectVasio8Loopback(vbmatrixHost);
    // Restart monitoring to re-establish audio path after routing changes
    await request.post("/api/v1/monitoring", {
      data: { enabled: false },
    });
    await new Promise((r) => setTimeout(r, 1000));
    await request.post("/api/v1/monitoring", {
      data: { enabled: true },
    });
    // Wait for signal to recover
    await new Promise((r) => setTimeout(r, 3000));
  });

  test("VBAN-TEXT communication with VBMatrix works", async () => {
    const available = await isVbanTextAvailable(vbmatrixHost);
    expect(available).toBe(true);
  });

  test("VASIO8 loopback routing points exist", async () => {
    const gain1 = await queryProperty(
      vbmatrixHost,
      "Point(VASIO8.IN[1],VASIO8.OUT[1]).dBGain",
    );
    const gain2 = await queryProperty(
      vbmatrixHost,
      "Point(VASIO8.IN[2],VASIO8.OUT[2]).dBGain",
    );
    // Routing points should exist (not -inf or Err)
    expect(gain1).not.toContain("Err");
    expect(gain2).not.toContain("Err");
  });

  test("signal_lost becomes true when loopback is disconnected", async ({
    request,
  }) => {
    // 1. Verify signal is OK before disconnect
    const before = await request.get("/api/v1/stats");
    const beforeStats = await before.json();
    expect(beforeStats.signal_lost).toBe(false);
    expect(beforeStats.current_latency).toBeLessThan(100);

    // 2. Disconnect VASIO8 loopback via VBAN-TEXT
    await disconnectVasio8Loopback(vbmatrixHost);

    // 3. Wait for signal detection to register the loss (up to 5 seconds)
    let signalLost = false;
    for (let i = 0; i < 10; i++) {
      await new Promise((r) => setTimeout(r, 500));
      const resp = await request.get("/api/v1/stats");
      const stats = await resp.json();
      if (stats.signal_lost) {
        signalLost = true;
        break;
      }
    }

    expect(signalLost).toBe(true);
  });

  test("latency shows aliased value when loopback is disconnected", async ({
    request,
  }) => {
    // Disconnect loopback
    await disconnectVasio8Loopback(vbmatrixHost);
    await new Promise((r) => setTimeout(r, 3000));

    const resp = await request.get("/api/v1/stats");
    const stats = await resp.json();

    // Without real loopback, latency should be > 100ms (MLS period aliasing)
    expect(stats.current_latency).toBeGreaterThan(100);
    expect(stats.signal_lost).toBe(true);
  });

  test("signal recovers after loopback is reconnected", async ({ request }) => {
    // 1. Disconnect loopback
    await disconnectVasio8Loopback(vbmatrixHost);
    await new Promise((r) => setTimeout(r, 3000));

    // Verify signal is lost
    const lostResp = await request.get("/api/v1/stats");
    const lostStats = await lostResp.json();
    expect(lostStats.signal_lost).toBe(true);

    // 2. Reconnect loopback
    await reconnectVasio8Loopback(vbmatrixHost);

    // 3. Restart monitoring to re-establish audio path
    await request.post("/api/v1/monitoring", {
      data: { enabled: false },
    });
    await new Promise((r) => setTimeout(r, 1000));
    await request.post("/api/v1/monitoring", {
      data: { enabled: true },
    });

    // 4. Wait for signal to recover (up to 10 seconds)
    let recovered = false;
    for (let i = 0; i < 20; i++) {
      await new Promise((r) => setTimeout(r, 500));
      const resp = await request.get("/api/v1/stats");
      const stats = await resp.json();
      if (!stats.signal_lost && stats.current_latency < 100) {
        recovered = true;
        break;
      }
    }

    expect(recovered).toBe(true);
  });

  test("dashboard shows NO SIGNAL when loopback disconnected", async ({
    page,
    request,
  }) => {
    await page.goto("/");
    await page.waitForTimeout(2000);

    // Verify Signal OK initially
    const signalEl = page.locator('[data-testid="signal-status"]');
    await expect(signalEl).toHaveText("Signal OK", { timeout: 5_000 });

    // Disconnect loopback
    await disconnectVasio8Loopback(vbmatrixHost);

    // Wait for dashboard to update via WebSocket
    await expect(signalEl).toHaveText("NO SIGNAL", { timeout: 10_000 });
  });
});
