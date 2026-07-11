import { dirname, join, resolve } from "node:path";
import { defineConfig } from "vite";
import { viteSingleFile } from "vite-plugin-singlefile";

const repositoryRoot = dirname(dirname(import.meta.dirname));

export default defineConfig({
  plugins: [viteSingleFile()],
  resolve: {
    alias: {
      "@budna-ui": resolve(import.meta.dirname, "src"),
    },
  },
  build: {
    assetsInlineLimit: Number.MAX_SAFE_INTEGER,
    cssCodeSplit: false,
    emptyOutDir: false,
    modulePreload: false,
    minify: "terser",
    outDir: join(repositoryRoot, "crates", "budna-mcp-server", "assets"),
    rollupOptions: {
      input: resolve(import.meta.dirname, "marketplace-explorer-v1.html"),
    },
    sourcemap: false,
    target: "es2022",
    terserOptions: {
      compress: {
        drop_console: true,
        passes: 3,
      },
      format: { comments: false },
      mangle: true,
    },
  },
});
