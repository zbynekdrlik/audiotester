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

  test("sample rate selection updates config via API", async ({
    page,
    request,
  }) => {
    // Select a different sample rate via the dropdown
    const sampleRate = page.getByLabel("Sample Rate");
    await sampleRate.selectOption("48000");

    // Verify via API that sample rate was updated
    // Allow time for the UI to send the update
    await page.waitForTimeout(1000);
    const resp = await request.get("/api/v1/config");
    expect(resp.ok()).toBeTruthy();
    const config = await resp.json();
    expect(config.sample_rate).toBe(48000);
  });

  test("start button initial state", async ({ page }) => {
    const startBtn = page.getByRole("button", { name: "Start" });
    await expect(startBtn).toBeVisible();
  });

  test("stop button initial state", async ({ page }) => {
    const stopBtn = page.getByRole("button", { name: "Stop" });
    await expect(stopBtn).toBeVisible();
  });

  test("navigates to dashboard", async ({ page }) => {
    await page.getByRole("link", { name: "Dashboard" }).click();
    await expect(page).toHaveTitle("Audiotester - Dashboard");
    await expect(page).toHaveURL("/");
  });
});
