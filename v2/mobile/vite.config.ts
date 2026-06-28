import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  build: {
    outDir: "build",
  },
  server: {
    port: 1421,
    strictPort: true,
  },
});
