import { test, expect } from "@playwright/test";

test.describe("Latency Timeline API", () => {
  test("GET /api/v1/latency-timeline returns valid response", async ({
    request,
  }) => {
    const resp = await request.get("/api/v1/latency-timeline");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body).toHaveProperty("bucket_size_secs");
    expect(body).toHaveProperty("buckets");
    expect(typeof body.bucket_size_secs).toBe("number");
    expect(Array.isArray(body.buckets)).toBe(true);
  });

  test("GET /api/v1/latency-timeline accepts range parameter", async ({
    request,
  }) => {
    const ranges = ["1h", "6h", "12h", "24h", "3d", "7d", "14d"];
    for (const range of ranges) {
      const resp = await request.get(`/api/v1/latency-timeline?range=${range}`);
      expect(resp.ok()).toBeTruthy();
      const body = await resp.json();
      expect(body).toHaveProperty("bucket_size_secs");
      expect(body).toHaveProperty("buckets");
    }
  });

  test("GET /api/v1/latency-timeline 1h range uses 10s buckets", async ({
    request,
  }) => {
    const resp = await request.get("/api/v1/latency-timeline?range=1h");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.bucket_size_secs).toBe(10);
  });

  test("GET /api/v1/latency-timeline 7d range uses 1800s buckets", async ({
    request,
  }) => {
    const resp = await request.get("/api/v1/latency-timeline?range=7d");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.bucket_size_secs).toBe(1800);
  });

  test("GET /api/v1/latency-timeline 14d range uses 3600s buckets", async ({
    request,
  }) => {
    const resp = await request.get("/api/v1/latency-timeline?range=14d");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.bucket_size_secs).toBe(3600);
  });

  test("GET /api/v1/latency-timeline buckets have correct shape", async ({
    request,
  }) => {
    const resp = await request.get("/api/v1/latency-timeline?range=1h");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    // Buckets may be empty if no data, but if present should have correct fields
    if (body.buckets.length > 0) {
      const bucket = body.buckets[0];
      expect(bucket).toHaveProperty("t");
      expect(bucket).toHaveProperty("avg");
      expect(bucket).toHaveProperty("min");
      expect(bucket).toHaveProperty("max");
      expect(typeof bucket.t).toBe("number");
      expect(typeof bucket.avg).toBe("number");
      expect(typeof bucket.min).toBe("number");
      expect(typeof bucket.max).toBe("number");
    }
  });
});

test.describe("Latency Timeline UI", () => {
  test("dashboard renders latency timeline container", async ({ page }) => {
    await page.goto("/");
    const container = page.locator('[data-testid="latency-timeline"]');
    await expect(container).toBeVisible();
  });

  test("dashboard renders latency zoom control buttons", async ({ page }) => {
    await page.goto("/");
    const controls = page.locator("#latency-zoom-controls");
    await expect(controls).toBeVisible();

    // Verify all seven zoom buttons exist
    const buttons = controls.locator(".zoom-btn");
    await expect(buttons).toHaveCount(7);

    // Verify button labels
    await expect(buttons.nth(0)).toHaveText("1h");
    await expect(buttons.nth(1)).toHaveText("6h");
    await expect(buttons.nth(2)).toHaveText("12h");
    await expect(buttons.nth(3)).toHaveText("24h");
    await expect(buttons.nth(4)).toHaveText("3d");
    await expect(buttons.nth(5)).toHaveText("7d");
    await expect(buttons.nth(6)).toHaveText("14d");
  });

  test("1h zoom button is active by default on latency timeline", async ({
    page,
  }) => {
    await page.goto("/");
    const btn1h = page.locator(
      '#latency-zoom-controls .zoom-btn[data-range="1h"]',
    );
    await expect(btn1h).toHaveClass(/active/);
  });

  test("clicking latency zoom button changes active state", async ({
    page,
  }) => {
    await page.goto("/");
    const btn1h = page.locator(
      '#latency-zoom-controls .zoom-btn[data-range="1h"]',
    );
    const btn14d = page.locator(
      '#latency-zoom-controls .zoom-btn[data-range="14d"]',
    );

    await btn14d.click();
    await expect(btn14d).toHaveClass(/active/);
    await expect(btn1h).not.toHaveClass(/active/);
  });
});
