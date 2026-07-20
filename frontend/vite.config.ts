import { defineConfig } from 'vite'
import react, { reactCompilerPreset } from '@vitejs/plugin-react'
import babel from '@rolldown/plugin-babel'

const backendTarget = 'http://127.0.0.1:7001'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    react(),
    babel({ presets: [reactCompilerPreset()] })
  ],
  server: {
    proxy: {
      '/api': backendTarget,
      '/health': backendTarget,
      '/v1': backendTarget,
      '/v1beta': backendTarget,
    },
  },
})
