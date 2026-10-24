import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

export default defineConfig({
  plugins: [svelte()],
  // MapLibre 6 loads its worker from a sibling module; pre-bundling breaks that URL
  optimizeDeps: { exclude: ['maplibre-gl'] },
  server: {
    // listen on all interfaces so phones/tablets on the LAN can use the dev server
    host: '0.0.0.0',
    port: 5173,
    allowedHosts: true,
    // the Rust server (run.sh) answers the API and serves the chart tiles
    proxy: { '/api': 'http://127.0.0.1:8787', '/tiles': 'http://127.0.0.1:8787' },
  },
  preview: { host: '0.0.0.0', port: 4173, proxy: { '/api': 'http://127.0.0.1:8787', '/tiles': 'http://127.0.0.1:8787' } },
})
