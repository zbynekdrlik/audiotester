import { test, expect } from "@playwright/test";

test.describe("Loss Timeline API", () => {
  test("GET /api/v1/loss-timeline returns valid response", async ({
    request,
  }) => {
    const resp = await request.get("/api/v1/loss-timeline");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body).toHaveProperty("bucket_size_secs");
    expect(body).toHaveProperty("buckets");
    expect(typeof body.bucket_size_secs).toBe("number");
    expect(Array.isArray(body.buckets)).toBe(true);
  });

  test("GET /api/v1/loss-timeline accepts range parameter", async ({
    request,
  }) => {
    const ranges = ["1h", "6h", "12h", "24h", "3d", "7d", "14d"];
    for (const range of ranges) {
      const resp = await request.get(`/api/v1/loss-timeline?range=${range}`);
      expect(resp.ok()).toBeTruthy();
      const body = await resp.json();
      expect(body).toHaveProperty("bucket_size_secs");
      expect(body).toHaveProperty("buckets");
    }
  });

  test("GET /api/v1/loss-timeline 1h range uses 10s buckets", async ({
    request,
  }) => {
    const resp = await request.get("/api/v1/loss-timeline?range=1h");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.bucket_size_secs).toBe(10);
  });

  test("GET /api/v1/loss-timeline 6h range uses 60s buckets", async ({
    request,
  }) => {
    const resp = await request.get("/api/v1/loss-timeline?range=6h");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.bucket_size_secs).toBe(60);
  });

  test("GET /api/v1/loss-timeline 24h range uses 300s buckets", async ({
    request,
  }) => {
    const resp = await request.get("/api/v1/loss-timeline?range=24h");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.bucket_size_secs).toBe(300);
  });

  test("GET /api/v1/loss-timeline 3d range uses 900s buckets", async ({
    request,
  }) => {
    const resp = await request.get("/api/v1/loss-timeline?range=3d");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.bucket_size_secs).toBe(900);
  });

  test("GET /api/v1/loss-timeline 7d range uses 1800s buckets", async ({
    request,
  }) => {
    const resp = await request.get("/api/v1/loss-timeline?range=7d");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.bucket_size_secs).toBe(1800);
  });

  test("GET /api/v1/loss-timeline 14d range uses 3600s buckets", async ({
    request,
  }) => {
    const resp = await request.get("/api/v1/loss-timeline?range=14d");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.bucket_size_secs).toBe(3600);
  });

  test("GET /api/v1/loss-timeline buckets have correct shape", async ({
    request,
  }) => {
    const resp = await request.get("/api/v1/loss-timeline?range=1h");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    // Buckets may be empty if no data, but if present should have correct fields
    if (body.buckets.length > 0) {
      const bucket = body.buckets[0];
      expect(bucket).toHaveProperty("t");
      expect(bucket).toHaveProperty("loss");
      expect(bucket).toHaveProperty("events");
      expect(typeof bucket.t).toBe("number");
      expect(typeof bucket.loss).toBe("number");
      expect(typeof bucket.events).toBe("number");
    }
  });
});

test.describe("Loss Timeline UI", () => {
  test("dashboard renders loss timeline container", async ({ page }) => {
    await page.goto("/");
    const container = page.locator('[data-testid="loss-timeline"]');
    await expect(container).toBeVisible();
  });

  test("dashboard renders zoom control buttons", async ({ page }) => {
    await page.goto("/");
    const controls = page.locator("#loss-zoom-controls");
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

  test("1h zoom button is active by default", async ({ page }) => {
    await page.goto("/");
    const btn1h = page.locator(
      '#loss-zoom-controls .zoom-btn[data-range="1h"]',
    );
    await expect(btn1h).toHaveClass(/active/);
  });

  test("clicking zoom button changes active state", async ({ page }) => {
    await page.goto("/");
    const btn1h = page.locator(
      '#loss-zoom-controls .zoom-btn[data-range="1h"]',
    );
    const btn14d = page.locator(
      '#loss-zoom-controls .zoom-btn[data-range="14d"]',
    );

    await btn14d.click();
    await expect(btn14d).toHaveClass(/active/);
    await expect(btn1h).not.toHaveClass(/active/);
  });

  test("loss timeline chart header shows title", async ({ page }) => {
    await page.goto("/");
    // The chart container should have a header with the title
    const header = page.locator(".chart-header");
    await expect(header.first()).toBeVisible();
  });
});
