import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { defineConfig } from 'vite';

// Until @kumikit/ui-sdk is published to npm, point KUMIKIT_UI_SDK at the web/sdk folder
// of a Kumikit checkout (defaults to ../../kumikit/web/sdk).
const sdk = process.env.KUMIKIT_UI_SDK ?? resolve(import.meta.dirname, '../../kumikit/web/sdk');

export default defineConfig({
  resolve: {
    alias: {
      '@kumikit/ui-sdk/panel': fileURLToPath(new URL('src/panel.ts', `file://${sdk.replace(/\/?$/, '/')}`)),
    },
  },
  build: {
    lib: { entry: 'src/panel.ts', formats: ['es'], fileName: () => 'panel.js' },
    outDir: 'package/dist',
    emptyOutDir: true,
    target: 'es2022',
    reportCompressedSize: false,
  },
});
