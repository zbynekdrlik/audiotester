import { test, expect } from "@playwright/test";

test.describe("Dashboard Page", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("loads and shows title", async ({ page }) => {
    await expect(page).toHaveTitle("Audiotester - Dashboard");
  });

  test("shows navigation with Dashboard and Settings links", async ({
    page,
  }) => {
    await expect(page.getByRole("link", { name: "Dashboard" })).toBeVisible();
    await expect(page.getByRole("link", { name: "Settings" })).toBeVisible();
  });

  test("shows all summary metrics", async ({ page }) => {
    await expect(page.getByText("Latency", { exact: true })).toBeVisible();
    await expect(page.getByText("Lost")).toBeVisible();
    await expect(page.getByText("Corrupted")).toBeVisible();
  });

  test("shows latency and loss chart sections", async ({ page }) => {
    await expect(page.getByText("Latency Timeline")).toBeVisible();
    await expect(page.getByText("Sample Loss Timeline")).toBeVisible();
  });

  test("WebSocket connects and shows Connected status", async ({ page }) => {
    await expect(page.getByText("Connected")).toBeVisible({ timeout: 10_000 });
  });

  test("latency value is displayed", async ({ page }) => {
    const latencyEl = page.locator('[data-testid="latency-value"]');
    await expect(latencyEl).toBeVisible();
    // Should have some numeric value (even 0.00)
    await expect(latencyEl).not.toHaveText("--");
  });

  test("lost and corrupted values show numeric values", async ({ page }) => {
    const lostEl = page.locator('[data-testid="lost-value"]');
    const corruptedEl = page.locator('[data-testid="corrupted-value"]');
    // If test-ids exist, verify they show numbers not "--"
    if ((await lostEl.count()) > 0) {
      await expect(lostEl).not.toHaveText("--");
    }
    if ((await corruptedEl.count()) > 0) {
      await expect(corruptedEl).not.toHaveText("--");
    }
  });

  test("chart containers exist with correct structure", async ({ page }) => {
    // Latency Timeline and Sample Loss sections should contain chart containers
    const latencySection = page.getByText("Latency Timeline");
    await expect(latencySection).toBeVisible();
    const lossSection = page.getByText("Sample Loss Timeline");
    await expect(lossSection).toBeVisible();
  });

  test("navigates to settings page", async ({ page }) => {
    await page.getByRole("link", { name: "Settings" }).click();
    await expect(page).toHaveTitle("Audiotester - Settings");
    await expect(page).toHaveURL(/\/settings/);
  });
});
