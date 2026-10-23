import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";
import mobileTsconfig from "./tsconfig.json";

export default defineConfig({
  plugins: [svelte()],
  // A string bypasses per-file tsconfig discovery for shared desktop fixtures,
  // which must not require generated desktop SvelteKit files in a mobile build.
  esbuild: { tsconfigRaw: JSON.stringify({ compilerOptions: mobileTsconfig.compilerOptions }) },
  clearScreen: false,
  build: {
    outDir: "build",
  },
  server: {
    port: 1421,
    strictPort: true,
  },
});
