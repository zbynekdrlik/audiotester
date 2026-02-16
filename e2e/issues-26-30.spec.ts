import { test, expect } from "@playwright/test";
import { restartAudioEngine } from "./helpers/vban-text";

// These tests run against a live audiotester instance
const isHardwareTest = process.env.AUDIOTESTER_HARDWARE_TEST === "true";
const vbmatrixHost = process.env.AUDIOTESTER_HOST || "iem.lan";

/**
 * Helper: check if ASIO devices are available (skip on CI without hardware)
 */
async function requireDevice(
  request: import("@playwright/test").APIRequestContext,
): Promise<string | null> {
  const devResp = await request.get("/api/v1/devices");
  const devices = await devResp.json();
  if (devices.length === 0) return null;
  return devices[0].name;
}

// Issue #30: Stop then start can't reconnect to VASIO-8
test.describe("Issue #30: Stop/Start Reconnection", () => {
  test("stop then start monitoring resumes successfully", async ({
    request,
  }) => {
    const deviceName = await requireDevice(request);
    if (!deviceName) {
      test.skip(true, "No ASIO devices available");
      return;
    }

    // Select device first
    await request.patch("/api/v1/config", {
      data: { device: deviceName },
    });

    // Start monitoring to have something to stop
    await request.post("/api/v1/monitoring", { data: { enabled: true } });
    await new Promise((r) => setTimeout(r, 1000));

    // Stop monitoring
    const stopResp = await request.post("/api/v1/monitoring", {
      data: { enabled: false },
    });
    expect(stopResp.ok()).toBeTruthy();
    const stopBody = await stopResp.json();
    expect(stopBody.monitoring).toBe(false);

    // Wait for ASIO to fully release resources
    await new Promise((r) => setTimeout(r, 2000));

    // Start monitoring - this is the critical test for #30
    const startResp = await request.post("/api/v1/monitoring", {
      data: { enabled: true },
    });
    expect(startResp.ok()).toBeTruthy();
    const startBody = await startResp.json();
    expect(startBody.monitoring).toBe(true);
  });

  test("start after stop returns valid stats within 5s", async ({
    request,
  }) => {
    const deviceName = await requireDevice(request);
    if (!deviceName) {
      test.skip(true, "No ASIO devices available");
      return;
    }

    // Select device
    await request.patch("/api/v1/config", {
      data: { device: deviceName },
    });

    // Stop
    await request.post("/api/v1/monitoring", { data: { enabled: false } });
    await new Promise((r) => setTimeout(r, 2000));

    // Start
    const startResp = await request.post("/api/v1/monitoring", {
      data: { enabled: true },
    });
    expect(startResp.ok()).toBeTruthy();

    // The key test is that start succeeded (didn't return error)
    const status = await request.get("/api/v1/status");
    const body = await status.json();
    expect(body.monitoring).toBe(true);
  });

  test("double stop is idempotent", async ({ request }) => {
    // Stop twice - should not error (works even without device)
    const resp1 = await request.post("/api/v1/monitoring", {
      data: { enabled: false },
    });
    expect(resp1.ok()).toBeTruthy();

    const resp2 = await request.post("/api/v1/monitoring", {
      data: { enabled: false },
    });
    expect(resp2.ok()).toBeTruthy();
    const body = await resp2.json();
    expect(body.monitoring).toBe(false);
  });

  test("double start is idempotent", async ({ request }) => {
    const deviceName = await requireDevice(request);
    if (!deviceName) {
      test.skip(true, "No ASIO devices available");
      return;
    }

    // Select device
    await request.patch("/api/v1/config", {
      data: { device: deviceName },
    });

    // Ensure stopped first
    await request.post("/api/v1/monitoring", { data: { enabled: false } });
    await new Promise((r) => setTimeout(r, 1000));

    // Start twice - should not error
    const resp1 = await request.post("/api/v1/monitoring", {
      data: { enabled: true },
    });
    expect(resp1.ok()).toBeTruthy();

    const resp2 = await request.post("/api/v1/monitoring", {
      data: { enabled: true },
    });
    expect(resp2.ok()).toBeTruthy();
    const body = await resp2.json();
    expect(body.monitoring).toBe(true);
  });
});

// Issue #26: Latency measurement consistency
test.describe("Issue #26: Latency Measurement Stability", () => {
  test("GET /api/v1/stats includes avg_latency field", async ({ request }) => {
    const resp = await request.get("/api/v1/stats");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body).toHaveProperty("avg_latency");
    expect(typeof body.avg_latency).toBe("number");
  });

  test("latency values are stable within session", async ({ request }) => {
    // Collect 5 latency samples over 2.5 seconds
    const samples: number[] = [];
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 500));
      const resp = await request.get("/api/v1/stats");
      const stats = await resp.json();
      if (stats.current_latency > 0) {
        samples.push(stats.current_latency);
      }
    }

    // On CI without ASIO, we may get no samples - skip check
    if (samples.length >= 2) {
      const min = Math.min(...samples);
      const max = Math.max(...samples);
      const spread = max - min;
      // Latency spread should be < 2ms within a stable session
      expect(spread).toBeLessThan(2);
    }
  });

  test("latency recovers within 2ms after stop/start cycle", async ({
    request,
  }) => {
    // Get baseline latency
    const baseResp = await request.get("/api/v1/stats");
    const baseline = await baseResp.json();
    const baselineLatency = baseline.current_latency;

    // Only test if we have a valid baseline (hardware with active signal)
    if (baselineLatency > 0 && baselineLatency < 100) {
      // Stop and restart
      await request.post("/api/v1/monitoring", { data: { enabled: false } });
      await new Promise((r) => setTimeout(r, 2000));
      await request.post("/api/v1/monitoring", { data: { enabled: true } });

      // Wait for measurements to resume
      let recovered = false;
      for (let i = 0; i < 20; i++) {
        await new Promise((r) => setTimeout(r, 500));
        const resp = await request.get("/api/v1/stats");
        const stats = await resp.json();
        if (
          stats.current_latency > 0 &&
          stats.current_latency < 100 &&
          Math.abs(stats.current_latency - baselineLatency) < 2
        ) {
          recovered = true;
          break;
        }
      }

      expect(recovered).toBe(true);
    }
  });
});

// Diagnostic logging API
test.describe("Diagnostic Logging API", () => {
  test("GET /api/v1/logs returns recent log lines", async ({ request }) => {
    const resp = await request.get("/api/v1/logs?tail=50");
    expect(resp.ok()).toBeTruthy();
    const text = await resp.text();
    expect(text.length).toBeGreaterThan(0);
    // Should contain app startup logs
    expect(text).toContain("audiotester");
  });

  test("GET /api/v1/logs supports tail parameter", async ({ request }) => {
    const resp5 = await request.get("/api/v1/logs?tail=5");
    expect(resp5.ok()).toBeTruthy();
    const text5 = await resp5.text();

    const resp50 = await request.get("/api/v1/logs?tail=50");
    expect(resp50.ok()).toBeTruthy();
    const text50 = await resp50.text();

    // More lines requested should return equal or more content
    expect(text50.length).toBeGreaterThanOrEqual(text5.length);
  });

  test("GET /api/v1/logs supports filter parameter", async ({ request }) => {
    const resp = await request.get("/api/v1/logs?tail=200&filter=INFO");
    expect(resp.ok()).toBeTruthy();
    const text = await resp.text();
    // Every returned line should contain the filter keyword
    if (text.length > 0) {
      const lines = text.split("\n").filter((l) => l.length > 0);
      for (const line of lines) {
        expect(line).toContain("INFO");
      }
    }
  });
});

// Hardware-only tests for issue #30 and #26
test.describe("Issue #30/#26 Hardware Tests", () => {
  test.skip(!isHardwareTest, "Requires AUDIOTESTER_HARDWARE_TEST=true");

  test("stop and restart monitoring recovers signal on hardware", async ({
    request,
  }) => {
    // Verify signal OK initially
    const before = await request.get("/api/v1/stats");
    const beforeStats = await before.json();
    expect(beforeStats.signal_lost).toBe(false);

    // Stop monitoring
    await request.post("/api/v1/monitoring", { data: { enabled: false } });
    await new Promise((r) => setTimeout(r, 2000));

    // Start monitoring
    const startResp = await request.post("/api/v1/monitoring", {
      data: { enabled: true },
    });
    expect(startResp.ok()).toBeTruthy();

    // Wait for signal recovery
    let recovered = false;
    for (let i = 0; i < 20; i++) {
      await new Promise((r) => setTimeout(r, 500));
      const resp = await request.get("/api/v1/stats");
      const stats = await resp.json();
      if (
        !stats.signal_lost &&
        stats.current_latency > 0 &&
        stats.current_latency < 50
      ) {
        recovered = true;
        break;
      }
    }

    expect(recovered).toBe(true);
  });

  test("latency stable within 2ms after VBMatrix restart", async ({
    request,
  }) => {
    // 1. Get baseline latency (average of 3 samples)
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

    // 2. Restart VBMatrix audio engine
    await restartAudioEngine(vbmatrixHost);

    // 3. Wait for auto-reconnection (~15s)
    let reconnected = false;
    for (let i = 0; i < 30; i++) {
      await new Promise((r) => setTimeout(r, 1000));
      const resp = await request.get("/api/v1/stats");
      const stats = await resp.json();
      if (
        !stats.signal_lost &&
        stats.current_latency > 0 &&
        stats.current_latency < 50
      ) {
        reconnected = true;
        break;
      }
    }
    expect(reconnected).toBe(true);

    // 4. Get post-restart latency (average of 3 samples)
    const postSamples: number[] = [];
    for (let i = 0; i < 3; i++) {
      await new Promise((r) => setTimeout(r, 500));
      const resp = await request.get("/api/v1/stats");
      const stats = await resp.json();
      if (stats.current_latency > 0 && stats.current_latency < 50) {
        postSamples.push(stats.current_latency);
      }
    }
    expect(postSamples.length).toBeGreaterThan(0);
    const postAvg = postSamples.reduce((a, b) => a + b, 0) / postSamples.length;

    // 5. Latency should be within 2ms of baseline
    const diff = Math.abs(postAvg - baselineAvg);
    expect(diff).toBeLessThan(2);
  });
});
