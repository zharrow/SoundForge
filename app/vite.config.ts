import { defineConfig } from "vite";

// Configuration attendue par Tauri : port fixe (1421, Pixel Forge garde le 1420), pas d'écran effacé, WebKit récent.
export default defineConfig({
  clearScreen: false,
  server: { port: 1421, strictPort: true },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: { target: "safari16", outDir: "dist", emptyOutDir: true },
});
