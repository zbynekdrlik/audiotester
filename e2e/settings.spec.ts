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

  test("shows Channel Pair section", async ({ page }) => {
    await expect(page.getByText("Channel Pair")).toBeVisible();
  });

  test("shows signal and counter channel dropdowns", async ({ page }) => {
    await expect(page.getByLabel("Signal Channel")).toBeVisible();
    await expect(page.getByLabel("Counter Channel")).toBeVisible();
  });

  test("channel dropdowns have default values", async ({ page }) => {
    // Wait for config to load
    await page.waitForTimeout(1000);
    const signalChannel = page.getByLabel("Signal Channel");
    const counterChannel = page.getByLabel("Counter Channel");
    // Defaults should be 1 and 2
    await expect(signalChannel).toHaveValue("1");
    await expect(counterChannel).toHaveValue("2");
  });

  test("channel pair API updates work", async ({ request }) => {
    // Set channel pair via API (avoids UI dropdown range issues on CI)
    const resp = await request.patch("/api/v1/config", {
      data: { channel_pair: [2, 1] },
    });
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.channel_pair).toEqual([2, 1]);

    // Verify it persists on GET
    const getResp = await request.get("/api/v1/config");
    const config = await getResp.json();
    expect(config.channel_pair).toEqual([2, 1]);

    // Reset to default
    await request.patch("/api/v1/config", {
      data: { channel_pair: [1, 2] },
    });
  });

  test("navigates to dashboard", async ({ page }) => {
    await page.getByRole("link", { name: "Dashboard" }).click();
    await expect(page).toHaveTitle("Audiotester - Dashboard");
    await expect(page).toHaveURL("/");
  });
});
