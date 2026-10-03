import { defineConfig } from "vitest/config"

// Unit tests don't need the app's Cloudflare worker or TanStack server plugins.
export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.{ts,tsx}"],
  },
})
