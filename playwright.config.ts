import { defineConfig } from "@playwright/test";

const isHardwareTest = process.env.AUDIOTESTER_HARDWARE_TEST === "true";
const host = process.env.AUDIOTESTER_HOST || "127.0.0.1";
const port = process.env.PORT || "8920";
const baseURL = `http://${host}:${port}`;

export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  retries: 0,
  use: {
    baseURL,
    headless: true,
  },
  // When running hardware tests against a remote host, skip starting a local server
  ...(isHardwareTest
    ? {}
    : {
        webServer: {
          command: `cargo run -p audiotester-server --bin test-server`,
          url: `http://127.0.0.1:${port}/api/v1/status`,
          reuseExistingServer: !!process.env.CI,
          timeout: 120_000,
          env: {
            PORT: port,
          },
        },
      }),
  projects: [
    {
      name: "chromium",
      use: { browserName: "chromium" },
    },
  ],
});
