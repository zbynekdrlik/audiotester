import { test, expect } from "@playwright/test";
import {
  isVbanTextAvailable,
  muteCounterChannel,
  unmuteCounterChannel,
  reconnectVasio8Loopback,
  removeCounterChannel,
  recreateCounterChannel,
} from "./helpers/vban-text";

// These tests only run when AUDIOTESTER_HARDWARE_TEST=true
// They require real ASIO hardware (VASIO 8 loopback on iem.lan)
const isHardwareTest = process.env.AUDIOTESTER_HARDWARE_TEST === "true";
const vbmatrixHost = process.env.AUDIOTESTER_HOST || "iem.lan";

test.describe("Counter Silence Detection (CH1 Mute)", () => {
  test.skip(!isHardwareTest, "Requires AUDIOTESTER_HARDWARE_TEST=true");

  // Ensure full loopback is restored after each test
  test.afterEach(async () => {
    await reconnectVasio8Loopback(vbmatrixHost);
    // Allow recovery time
    await new Promise((r) => setTimeout(r, 3000));
  });

  test("1: VBAN per-channel control works", async () => {
    const available = await isVbanTextAvailable(vbmatrixHost);
    expect(available).toBe(true);

    // Mute only ch2 (counter channel)
    await muteCounterChannel(vbmatrixHost);
    await new Promise((r) => setTimeout(r, 500));

    // Unmute ch2
    await unmuteCounterChannel(vbmatrixHost);
    // If no error, per-channel control works
  });

  test("2: diagnostic - what VBMatrix mute sends on ch1", async ({
    request,
  }) => {
    // Baseline: record stats before mute
    const beforeResp = await request.get("/api/v1/stats");
    const before = await beforeResp.json();
    console.log("BEFORE MUTE:", {
      counter_silent: before.counter_silent,
      estimated_loss: before.estimated_loss,
      total_lost: before.total_lost,
      signal_lost: before.signal_lost,
      confidence: before.confidence,
    });

    // Mute only ch2 (counter channel)
    await muteCounterChannel(vbmatrixHost);

    // Poll API for 10 seconds and log values
    for (let i = 0; i < 20; i++) {
      await new Promise((r) => setTimeout(r, 500));
      const resp = await request.get("/api/v1/stats");
      const stats = await resp.json();
      console.log(`DURING MUTE [${(i + 1) * 0.5}s]:`, {
        counter_silent: stats.counter_silent,
        estimated_loss: stats.estimated_loss,
        total_lost: stats.total_lost,
        signal_lost: stats.signal_lost,
        confidence: stats.confidence,
        current_latency: stats.current_latency,
      });
    }

    // This test always passes - it's diagnostic
    expect(true).toBe(true);
  });

  test("3: counter_silent=true when ch2 muted", async ({ request }) => {
    // Verify baseline: counter not silent
    const beforeResp = await request.get("/api/v1/stats");
    const before = await beforeResp.json();
    expect(before.counter_silent).toBe(false);

    // Mute only counter channel
    await muteCounterChannel(vbmatrixHost);

    // Wait for counter silence to be detected (up to 10s)
    let detected = false;
    for (let i = 0; i < 20; i++) {
      await new Promise((r) => setTimeout(r, 500));
      const resp = await request.get("/api/v1/stats");
      const stats = await resp.json();
      if (stats.counter_silent) {
        detected = true;
        break;
      }
    }

    expect(detected).toBe(true);
  });

  test("4: estimated_loss increases during mute", async ({ request }) => {
    // Mute counter channel
    await muteCounterChannel(vbmatrixHost);

    // Wait for silence detection
    let silenceDetected = false;
    for (let i = 0; i < 20; i++) {
      await new Promise((r) => setTimeout(r, 500));
      const resp = await request.get("/api/v1/stats");
      const stats = await resp.json();
      if (stats.counter_silent) {
        silenceDetected = true;
        break;
      }
    }
    expect(silenceDetected).toBe(true);

    // Record estimated_loss at two time points
    const resp1 = await request.get("/api/v1/stats");
    const stats1 = await resp1.json();
    const est1 = stats1.estimated_loss;

    await new Promise((r) => setTimeout(r, 2000));

    const resp2 = await request.get("/api/v1/stats");
    const stats2 = await resp2.json();
    const est2 = stats2.estimated_loss;

    // estimated_loss should be growing
    expect(est2).toBeGreaterThan(est1);
    // Should be roughly proportional to sample_rate * elapsed_time
    expect(est2).toBeGreaterThan(0);
  });

  test("5: estimated_loss rolls into total_lost on unmute", async ({
    request,
  }) => {
    // Mute counter channel and wait for silence detection
    await muteCounterChannel(vbmatrixHost);

    let silenceDetected = false;
    for (let i = 0; i < 20; i++) {
      await new Promise((r) => setTimeout(r, 500));
      const resp = await request.get("/api/v1/stats");
      const stats = await resp.json();
      if (stats.counter_silent) {
        silenceDetected = true;
        break;
      }
    }
    expect(silenceDetected).toBe(true);

    // Wait 3s for estimated_loss to accumulate meaningfully
    await new Promise((r) => setTimeout(r, 3000));

    // Record total_lost and estimated_loss just before unmute
    const beforeUnmute = await request.get("/api/v1/stats");
    const beforeStats = await beforeUnmute.json();
    const lostBefore = beforeStats.total_lost;
    const estimatedBefore = beforeStats.estimated_loss;
    expect(estimatedBefore).toBeGreaterThan(0);

    // Unmute counter channel
    await unmuteCounterChannel(vbmatrixHost);

    // Wait for recovery (up to 5s)
    let recovered = false;
    for (let i = 0; i < 10; i++) {
      await new Promise((r) => setTimeout(r, 500));
      const resp = await request.get("/api/v1/stats");
      const stats = await resp.json();
      if (!stats.counter_silent && stats.estimated_loss === 0) {
        recovered = true;
        // estimated_loss should have been added to total_lost
        // Allow 20% tolerance for timing differences
        const increase = stats.total_lost - lostBefore;
        expect(increase).toBeGreaterThanOrEqual(estimatedBefore * 0.8);
        break;
      }
    }

    expect(recovered).toBe(true);
  });

  test("6: ch0 latency independent during ch2 mute", async ({ request }) => {
    // Get baseline latency (average of 3 samples)
    const baselines: number[] = [];
    for (let i = 0; i < 3; i++) {
      await new Promise((r) => setTimeout(r, 500));
      const resp = await request.get("/api/v1/stats");
      const stats = await resp.json();
      if (stats.current_latency > 0 && stats.current_latency < 50) {
        baselines.push(stats.current_latency);
      }
    }
    expect(baselines.length).toBeGreaterThan(0);
    const baselineAvg = baselines.reduce((a, b) => a + b, 0) / baselines.length;

    // Mute only counter channel (ch0 burst should be unaffected)
    await muteCounterChannel(vbmatrixHost);
    await new Promise((r) => setTimeout(r, 3000));

    // Latency should still be valid (ch0 independent)
    const postSamples: number[] = [];
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 500));
      const resp = await request.get("/api/v1/stats");
      const stats = await resp.json();
      if (stats.current_latency > 0 && stats.current_latency < 50) {
        postSamples.push(stats.current_latency);
      }
    }
    expect(postSamples.length).toBeGreaterThan(0);
    const postAvg = postSamples.reduce((a, b) => a + b, 0) / postSamples.length;

    // Latency should be within 2ms of baseline
    const diff = Math.abs(postAvg - baselineAvg);
    expect(diff).toBeLessThan(2);

    // signal_lost should NOT be true (ch0 burst still active)
    const finalResp = await request.get("/api/v1/stats");
    const finalStats = await finalResp.json();
    expect(finalStats.signal_lost).toBe(false);
  });

  test("7: dashboard shows estimated loss during mute", async ({
    page,
    request,
  }) => {
    await page.goto("/");
    await page.waitForTimeout(2000);

    // Mute counter channel
    await muteCounterChannel(vbmatrixHost);

    // Wait for counter silence to be detected
    let silenceDetected = false;
    for (let i = 0; i < 20; i++) {
      await new Promise((r) => setTimeout(r, 500));
      const resp = await request.get("/api/v1/stats");
      const stats = await resp.json();
      if (stats.counter_silent) {
        silenceDetected = true;
        break;
      }
    }
    expect(silenceDetected).toBe(true);

    // Check that estimated-loss element is visible in the dashboard
    const estimatedLossEl = page.locator('[data-testid="estimated-loss"]');
    await expect(estimatedLossEl).toBeVisible({ timeout: 5000 });

    // Should contain "est." text
    const text = await estimatedLossEl.textContent();
    expect(text).toContain("est.");
  });

  test("8: dashboard hides estimated loss after unmute", async ({
    page,
    request,
  }) => {
    await page.goto("/");
    await page.waitForTimeout(2000);

    // Mute counter channel and wait for detection
    await muteCounterChannel(vbmatrixHost);

    let silenceDetected = false;
    for (let i = 0; i < 20; i++) {
      await new Promise((r) => setTimeout(r, 500));
      const resp = await request.get("/api/v1/stats");
      const stats = await resp.json();
      if (stats.counter_silent) {
        silenceDetected = true;
        break;
      }
    }
    expect(silenceDetected).toBe(true);

    // Verify estimated-loss is visible
    const estimatedLossEl = page.locator('[data-testid="estimated-loss"]');
    await expect(estimatedLossEl).toBeVisible({ timeout: 5000 });

    // Unmute counter channel
    await unmuteCounterChannel(vbmatrixHost);

    // Wait for estimated loss to disappear (up to 5s)
    await expect(estimatedLossEl).toBeHidden({ timeout: 10000 });

    // Lost value should return to normal styling
    const lostEl = page.locator('[data-testid="lost-value"]');
    const className = await lostEl.getAttribute("class");
    expect(className).not.toContain("warning");
  });

  test("9: lost value color is orange during mute", async ({
    page,
    request,
  }) => {
    await page.goto("/");
    await page.waitForTimeout(2000);

    // Mute counter channel and wait for detection
    await muteCounterChannel(vbmatrixHost);

    let silenceDetected = false;
    for (let i = 0; i < 20; i++) {
      await new Promise((r) => setTimeout(r, 500));
      const resp = await request.get("/api/v1/stats");
      const stats = await resp.json();
      if (stats.counter_silent) {
        silenceDetected = true;
        break;
      }
    }
    expect(silenceDetected).toBe(true);

    // Wait for dashboard to update via WebSocket
    await page.waitForTimeout(1000);

    // Lost value should have "warning" class (orange)
    const lostEl = page.locator('[data-testid="lost-value"]');
    await expect(lostEl).toHaveClass(/warning/, { timeout: 5000 });
  });

  test("10: 3 mute/unmute cycles accumulate total_lost", async ({
    request,
  }) => {
    // Record baseline total_lost
    const baseResp = await request.get("/api/v1/stats");
    const baseStats = await baseResp.json();
    let cumulativeLost = baseStats.total_lost;

    for (let cycle = 0; cycle < 3; cycle++) {
      // Mute counter channel
      await muteCounterChannel(vbmatrixHost);

      // Wait for silence detection
      let silenceDetected = false;
      for (let i = 0; i < 20; i++) {
        await new Promise((r) => setTimeout(r, 500));
        const resp = await request.get("/api/v1/stats");
        const stats = await resp.json();
        if (stats.counter_silent) {
          silenceDetected = true;
          break;
        }
      }
      expect(silenceDetected).toBe(true);

      // Let estimated_loss accumulate for 2s
      await new Promise((r) => setTimeout(r, 2000));

      // Record state before unmute
      const beforeResp = await request.get("/api/v1/stats");
      const beforeStats = await beforeResp.json();
      const lostBefore = beforeStats.total_lost;
      const estimatedBefore = beforeStats.estimated_loss;
      expect(estimatedBefore).toBeGreaterThan(0);

      // Unmute counter channel
      await unmuteCounterChannel(vbmatrixHost);

      // Wait for recovery
      let recovered = false;
      for (let i = 0; i < 10; i++) {
        await new Promise((r) => setTimeout(r, 500));
        const resp = await request.get("/api/v1/stats");
        const stats = await resp.json();
        if (!stats.counter_silent && stats.estimated_loss === 0) {
          recovered = true;
          // Estimated loss should roll into total_lost
          const increase = stats.total_lost - lostBefore;
          expect(increase).toBeGreaterThanOrEqual(estimatedBefore * 0.8);
          cumulativeLost = stats.total_lost;
          break;
        }
      }
      expect(recovered).toBe(true);

      // Brief stabilization between cycles
      await new Promise((r) => setTimeout(r, 2000));
    }

    // After 3 cycles, total_lost should be significantly higher than baseline
    expect(cumulativeLost).toBeGreaterThan(baseStats.total_lost);
  });

  test("11: routing point removal does NOT trigger engine restart", async ({
    request,
  }) => {
    // Get baseline: ch0 latency should be valid
    const baseResp = await request.get("/api/v1/stats");
    const baseStats = await baseResp.json();
    expect(baseStats.current_latency).toBeGreaterThan(0);
    expect(baseStats.current_latency).toBeLessThan(50);
    expect(baseStats.signal_lost).toBe(false);

    // Remove the counter channel routing point entirely (not just mute)
    await removeCounterChannel(vbmatrixHost);

    // Monitor for 15s: ch0 latency should remain stable throughout.
    // If the old lost_samples ASIO restart fires, latency will drop to 0
    // and signal_lost will become true (engine stops for 15s settle time).
    let restartDetected = false;
    const latencies: number[] = [];
    for (let i = 0; i < 30; i++) {
      await new Promise((r) => setTimeout(r, 500));
      const resp = await request.get("/api/v1/stats");
      const stats = await resp.json();
      latencies.push(stats.current_latency);

      // A latency of 0 or >100ms would indicate engine restart/cycling
      if (stats.current_latency === 0 || stats.current_latency > 100) {
        // Allow first 2s for transition
        if (i > 4) {
          restartDetected = true;
        }
      }
    }

    // Engine should NOT have restarted
    expect(restartDetected).toBe(false);

    // Signal should still be valid (ch0 burst is independent)
    const finalResp = await request.get("/api/v1/stats");
    const finalStats = await finalResp.json();
    expect(finalStats.signal_lost).toBe(false);

    // Recreate the routing point for cleanup
    await recreateCounterChannel(vbmatrixHost);
    await new Promise((r) => setTimeout(r, 3000));
  });
});
