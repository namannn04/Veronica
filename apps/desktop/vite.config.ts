import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri connects to a fixed dev-server address, so neither the port nor the
// host may be chosen automatically.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5183,
    strictPort: true,
    host: "127.0.0.1",
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    // WebKitGTK 2.52 handles modern syntax, so no downlevelling is needed.
    target: "esnext",
    minify: "esbuild",
    sourcemap: false,
  },
});
