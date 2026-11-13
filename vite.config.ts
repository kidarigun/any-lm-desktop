/// <reference types="vitest/config" />
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { cpSync, createReadStream, existsSync, statSync } from "node:fs";
import { resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const host = process.env.TAURI_DEV_HOST;
const rootDir = fileURLToPath(new URL(".", import.meta.url));

/**
 * pdf.js가 런타임에 읽는 보조 파일(CMap, 표준 글꼴, wasm 디코더)을 `/pdfjs/` 경로로 제공한다.
 * 개발 서버에서는 node_modules에서 바로 읽고, 빌드 시에는 dist/pdfjs로 복사한다.
 */
function pdfjsAssets(): Plugin {
  const pkgRoot = resolve(rootDir, "node_modules/pdfjs-dist");
  const dirs = ["cmaps", "standard_fonts", "wasm"];
  return {
    name: "pdfjs-assets",
    configureServer(server) {
      server.middlewares.use("/pdfjs", (req, res, next) => {
        const rel = decodeURIComponent((req.url || "").split("?")[0]);
        const file = resolve(pkgRoot, "." + rel);
        const allowed = dirs.some((d) => file.startsWith(resolve(pkgRoot, d) + sep));
        if (!allowed || !existsSync(file) || !statSync(file).isFile()) return next();
        res.setHeader("Content-Type", file.endsWith(".wasm") ? "application/wasm" : "application/octet-stream");
        createReadStream(file).pipe(res);
      });
    },
    writeBundle(options) {
      const outDir = options.dir || resolve(rootDir, "dist");
      for (const d of dirs) {
        const src = resolve(pkgRoot, d);
        if (existsSync(src)) cpSync(src, resolve(outDir, "pdfjs", d), { recursive: true });
      }
    },
  };
}

export default defineConfig(async () => ({
  plugins: [react(), tailwindcss(), pdfjsAssets()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
    testTimeout: 30000,
  },
}));
