import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  retries: 0,
  use: {
    baseURL: `http://127.0.0.1:${process.env.PORT || 8920}`,
    headless: true,
  },
  webServer: {
    command: `cargo run -p audiotester-server --bin test-server`,
    url: `http://127.0.0.1:${process.env.PORT || 8920}/api/v1/status`,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
    env: {
      PORT: process.env.PORT || "8920",
    },
  },
  projects: [
    {
      name: "chromium",
      use: { browserName: "chromium" },
    },
  ],
});
