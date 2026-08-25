import { defineConfig } from "vite";

export default defineConfig({
  base: "./",
  resolve: {
    extensions: [".ts", ".js", ".json"],
  },
  json: {
    stringify: false,
  },
  build: {
    rollupOptions: {
      external: ["/wasm/compiler.mjs"],
    },
  },
});
