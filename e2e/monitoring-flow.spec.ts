import { test, expect } from "@playwright/test";

test.describe("Monitoring Flow", () => {
  test("GET /api/v1/status shows initial Stopped state", async ({
    request,
  }) => {
    const resp = await request.get("/api/v1/status");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.state).toBe("Stopped");
    expect(body.monitoring).toBe(false);
  });

  test("PATCH config with device and start monitoring", async ({ request }) => {
    // Get available devices
    const devResp = await request.get("/api/v1/devices");
    expect(devResp.ok()).toBeTruthy();
    const devices = await devResp.json();

    // Skip if no ASIO devices available (CI environment)
    if (devices.length === 0) {
      test.skip(true, "No ASIO devices available");
      return;
    }

    const deviceName = devices[0].name;

    // Configure device
    const configResp = await request.patch("/api/v1/config", {
      data: { device: deviceName, sample_rate: 48000 },
    });
    expect(configResp.ok()).toBeTruthy();
    const config = await configResp.json();
    expect(config.device).toBe(deviceName);
    expect(config.sample_rate).toBe(48000);

    // Start monitoring
    const startResp = await request.post("/api/v1/monitoring", {
      data: { enabled: true },
    });
    expect(startResp.ok()).toBeTruthy();
    const startBody = await startResp.json();
    expect(startBody.monitoring).toBe(true);
    expect(startBody.state).toBe("Running");

    // Stop monitoring
    const stopResp = await request.post("/api/v1/monitoring", {
      data: { enabled: false },
    });
    expect(stopResp.ok()).toBeTruthy();
    const stopBody = await stopResp.json();
    expect(stopBody.monitoring).toBe(false);
    expect(stopBody.state).toBe("Stopped");
  });

  test("stop when already stopped is idempotent", async ({ request }) => {
    const resp = await request.post("/api/v1/monitoring", {
      data: { enabled: false },
    });
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.monitoring).toBe(false);
  });

  test("PATCH config with invalid device returns 400", async ({ request }) => {
    const resp = await request.patch("/api/v1/config", {
      data: { device: "NonExistent Device That Does Not Exist" },
    });
    expect(resp.status()).toBe(400);
  });

  test("PATCH config updates sample rate without device", async ({
    request,
  }) => {
    const resp = await request.patch("/api/v1/config", {
      data: { sample_rate: 96000 },
    });
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.sample_rate).toBe(96000);
  });
});
