import { test, expect } from "@playwright/test";

test.describe("Settings Page", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/settings");
  });

  test("loads and shows title", async ({ page }) => {
    await expect(page).toHaveTitle("Audiotester - Settings");
  });

  test("shows Audio Device section", async ({ page }) => {
    await expect(page.getByText("Audio Device")).toBeVisible();
  });

  test("shows device dropdown", async ({ page }) => {
    await expect(page.getByLabel("Device")).toBeVisible();
  });

  test("shows sample rate dropdown with options", async ({ page }) => {
    const sampleRate = page.getByLabel("Sample Rate");
    await expect(sampleRate).toBeVisible();
    // Verify default options are present
    await expect(sampleRate.locator("option")).toHaveCount(6);
  });

  test("shows Monitoring section with start/stop buttons", async ({ page }) => {
    await expect(page.getByText("Monitoring")).toBeVisible();
    await expect(page.getByRole("button", { name: "Start" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Stop" })).toBeVisible();
  });

  test("shows Device Information section", async ({ page }) => {
    await expect(page.getByText("Device Information")).toBeVisible();
  });

  test("navigates to dashboard", async ({ page }) => {
    await page.getByRole("link", { name: "Dashboard" }).click();
    await expect(page).toHaveTitle("Audiotester - Dashboard");
    await expect(page).toHaveURL("/");
  });
});
