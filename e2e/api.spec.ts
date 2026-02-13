import { test, expect } from "@playwright/test";

test.describe("REST API", () => {
  test("GET /api/v1/status returns valid status", async ({ request }) => {
    const resp = await request.get("/api/v1/status");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body).toHaveProperty("version");
    expect(body).toHaveProperty("state");
    expect(body).toHaveProperty("sample_rate");
    expect(body).toHaveProperty("monitoring");
    expect(typeof body.version).toBe("string");
    expect(typeof body.sample_rate).toBe("number");
    expect(typeof body.monitoring).toBe("boolean");
  });

  test("GET /api/v1/stats returns valid stats", async ({ request }) => {
    const resp = await request.get("/api/v1/stats");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body).toHaveProperty("current_latency");
    expect(body).toHaveProperty("min_latency");
    expect(body).toHaveProperty("max_latency");
    expect(body).toHaveProperty("avg_latency");
    expect(body).toHaveProperty("total_lost");
    expect(body).toHaveProperty("total_corrupted");
    expect(body).toHaveProperty("measurement_count");
    expect(body).toHaveProperty("latency_history");
    expect(body).toHaveProperty("loss_history");
    expect(Array.isArray(body.latency_history)).toBe(true);
    expect(Array.isArray(body.loss_history)).toBe(true);
  });

  test("GET /api/v1/devices returns device array", async ({ request }) => {
    const resp = await request.get("/api/v1/devices");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(Array.isArray(body)).toBe(true);
    // On CI/Linux there may be no ASIO devices, that's OK
  });

  test("GET /api/v1/config returns valid config", async ({ request }) => {
    const resp = await request.get("/api/v1/config");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body).toHaveProperty("sample_rate");
    expect(body).toHaveProperty("monitoring");
    expect(typeof body.sample_rate).toBe("number");
    expect(typeof body.monitoring).toBe("boolean");
  });

  test("PATCH /api/v1/config updates sample rate", async ({ request }) => {
    const resp = await request.patch("/api/v1/config", {
      data: { sample_rate: 48000 },
    });
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.sample_rate).toBe(48000);
  });

  test("POST /api/v1/monitoring accepts toggle request", async ({
    request,
  }) => {
    // Starting without a device may fail (500) or succeed depending on platform
    const resp = await request.post("/api/v1/monitoring", {
      data: { enabled: true },
    });
    // Accept both success and server error (no device on CI)
    expect([200, 500]).toContain(resp.status());
  });

  test("GET /api/v1/ws WebSocket endpoint exists", async ({ request }) => {
    // Just verify the endpoint doesn't 404 (it will fail upgrade without WS headers)
    const resp = await request.get("/api/v1/ws");
    // WebSocket endpoint returns non-200 for regular HTTP, but not 404
    expect(resp.status()).not.toBe(404);
  });
});
