import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
import tailwindcss from "@tailwindcss/vite";
import process from "node:process";
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  // Tailwind v4 runs as a Vite plugin; it compiles src/app.css (the shadcn-svelte token bridge).
  plugins: [tailwindcss(), sveltekit()],

  test: {
    environment: "jsdom",
  },

  // Vitest must resolve Svelte's browser build for component tests; the SvelteKit build sets its
  // own conditions per environment, so this only applies while tests run.
  ...(process.env.VITEST ? { resolve: { conditions: ["browser"] } } : {}),

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
