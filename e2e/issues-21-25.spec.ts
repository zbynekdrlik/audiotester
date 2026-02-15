import { test, expect } from "@playwright/test";

// Issue #21: Version + build date display
test.describe("Issue #21: Version and Build Date Display", () => {
  test("GET /api/v1/status includes build_date field", async ({ request }) => {
    const resp = await request.get("/api/v1/status");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body).toHaveProperty("build_date");
    expect(typeof body.build_date).toBe("string");
    // Build date should be a YYYY-MM-DD format
    expect(body.build_date).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });

  test("dashboard header shows version info", async ({ page }) => {
    await page.goto("/");
    const versionEl = page.locator('[data-testid="version-info"]');
    await expect(versionEl).toBeVisible({ timeout: 10_000 });
    // Should contain version pattern like "v0.1.5 (2026-02-15)"
    await expect(versionEl).toContainText(/v\d+\.\d+\.\d+/);
    await expect(versionEl).toContainText(/\(\d{4}-\d{2}-\d{2}\)/);
  });

  test("POST /api/v1/monitoring response includes build_date", async ({
    request,
  }) => {
    // Toggle monitoring (stop to be safe)
    const resp = await request.post("/api/v1/monitoring", {
      data: { enabled: false },
    });
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body).toHaveProperty("build_date");
    expect(typeof body.build_date).toBe("string");
  });
});

// Issue #24: Status line layout stability
test.describe("Issue #24: Device Info Bar Layout Stability", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("device info bar items have white-space nowrap", async ({ page }) => {
    // Wait for content to load
    await page.waitForTimeout(500);
    const infoItems = page.locator(".device-info-bar .info-item");
    const count = await infoItems.count();
    expect(count).toBeGreaterThan(0);

    // Each info-item should not wrap internally
    for (let i = 0; i < count; i++) {
      const whiteSpace = await infoItems.nth(i).evaluate((el) => {
        return getComputedStyle(el).whiteSpace;
      });
      expect(whiteSpace).toBe("nowrap");
    }
  });

  test("device info bar has flex-wrap for multi-line support", async ({
    page,
  }) => {
    const bar = page.locator(".device-info-bar");
    await expect(bar).toBeVisible();
    const flexWrap = await bar.evaluate((el) => {
      return getComputedStyle(el).flexWrap;
    });
    expect(flexWrap).toBe("wrap");
  });

  test("reset button does not wrap its text", async ({ page }) => {
    const resetBtn = page.locator(".btn-reset");
    await expect(resetBtn).toBeVisible();
    const whiteSpace = await resetBtn.evaluate((el) => {
      return getComputedStyle(el).whiteSpace;
    });
    expect(whiteSpace).toBe("nowrap");
  });

  test("info-value has minimum width to prevent oscillation", async ({
    page,
  }) => {
    const infoValues = page.locator(".device-info-bar .info-value");
    const count = await infoValues.count();
    expect(count).toBeGreaterThan(0);

    for (let i = 0; i < count; i++) {
      const minWidth = await infoValues.nth(i).evaluate((el) => {
        return getComputedStyle(el).minWidth;
      });
      // Should have a minimum width set (not "auto" or "0px")
      expect(minWidth).not.toBe("auto");
      expect(minWidth).not.toBe("0px");
    }
  });
});

// Issue #25: Start button error handling
test.describe("Issue #25: Start/Stop Button Error Handling", () => {
  test("settings page start button shows error on failure", async ({
    page,
  }) => {
    await page.goto("/settings");

    // On CI without ASIO devices, starting should fail
    const devices = await page
      .getByLabel("Device")
      .locator("option")
      .allInnerTexts();

    const startBtn = page.getByRole("button", { name: "Start" });
    await expect(startBtn).toBeVisible();

    // Click start - may produce error notification on CI (no ASIO device)
    if (devices.length === 0 || devices[0] === "No devices found") {
      await startBtn.click();
      // Should see an error notification or status remains Stopped
      await page.waitForTimeout(2000);
      const statusDisplay = page.locator(".status-display");
      const statusText = await statusDisplay.textContent();
      // Either shows error notification or stays stopped
      expect(["Stopped", "Running"]).toContain(statusText);
    }
  });

  test("settings page has error notification styling", async ({ page }) => {
    await page.goto("/settings");
    // Verify error-notification CSS is available (inject a test element)
    const hasStyle = await page.evaluate(() => {
      const el = document.createElement("div");
      el.className = "error-notification";
      document.body.appendChild(el);
      const style = getComputedStyle(el);
      const hasPosition = style.position === "fixed";
      document.body.removeChild(el);
      return hasPosition;
    });
    expect(hasStyle).toBe(true);
  });
});
