import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@veronica/extension-api": fileURLToPath(
        new URL("../../packages/extension-api/src/index.ts", import.meta.url),
      ),
      "@veronica/ui": fileURLToPath(new URL("../../packages/ui/index.ts", import.meta.url)),
    },
    dedupe: [
      "react",
      "react-dom",
      "lucide-react",
      "@fontsource-variable/inter",
      "@fontsource-variable/jetbrains-mono",
    ],
  },
  clearScreen: false,
  server: {
    port: 5183,
    strictPort: true,
    host: "127.0.0.1",
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "esnext",
    minify: "esbuild",
    sourcemap: false,
  },
});
