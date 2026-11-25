import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: [
      {
        find: /^@veronica\/extension-api$/,
        replacement: fileURLToPath(
          new URL("../../packages/extension-api/src/index.ts", import.meta.url),
        ),
      },
      {
        find: /^@veronica\/ui$/,
        replacement: fileURLToPath(new URL("../../packages/ui/index.ts", import.meta.url)),
      },
      {
        find: /^@veronica\/ui\/(.*)$/,
        replacement: `${fileURLToPath(new URL("../../packages/ui", import.meta.url))}/$1`,
      },
    ],
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
