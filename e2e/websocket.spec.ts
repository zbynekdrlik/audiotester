import { test, expect } from "@playwright/test";

test.describe("WebSocket", () => {
  test("receives stats updates via WebSocket on dashboard", async ({
    page,
  }) => {
    await page.goto("/");

    // WebSocket should connect and show Connected status
    await expect(page.getByText("Connected")).toBeVisible({ timeout: 10_000 });

    // The latency value should update from initial "--" to a numeric value
    const latencyEl = page.locator('[data-testid="latency-value"]');
    await expect(latencyEl).toBeVisible();
    await expect(latencyEl).not.toHaveText("--", { timeout: 10_000 });
  });

  test("reconnects WebSocket after page navigation", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByText("Connected")).toBeVisible({ timeout: 10_000 });

    // Navigate to settings
    await page.getByRole("link", { name: "Settings" }).click();
    await expect(page).toHaveURL(/\/settings/);

    // Navigate back to dashboard
    await page.getByRole("link", { name: "Dashboard" }).click();
    await expect(page).toHaveURL("/");

    // WebSocket should reconnect
    await expect(page.getByText("Connected")).toBeVisible({ timeout: 10_000 });
  });
});
