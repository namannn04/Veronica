import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const shared = (path: string) =>
  fileURLToPath(new URL(`../../../packages/${path}`, import.meta.url));

export default defineConfig({
  plugins: [react()],
  define: { "process.env.NODE_ENV": JSON.stringify("production") },
  resolve: {
    alias: [
      {
        find: /^@veronica\/extension-api$/,
        replacement: shared("extension-api/src/index.ts"),
      },
      { find: /^@veronica\/ui$/, replacement: shared("ui/index.ts") },
      { find: /^@veronica\/ui\/(.*)$/, replacement: shared("ui/$1") },
    ],
    dedupe: [
      "react",
      "react-dom",
      "lucide-react",
      "@fontsource-variable/inter",
      "@fontsource-variable/jetbrains-mono",
    ],
  },
  build: {
    target: "es2022",
    cssCodeSplit: false,
    emptyOutDir: true,
    lib: {
      entry: "src/main.tsx",
      formats: ["iife"],
      name: "extension",
      fileName: () => "index.js",
      cssFileName: "index",
    },
  },
});
