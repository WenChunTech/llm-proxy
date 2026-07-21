import { useEffect, useState } from 'react'
import { readStoredThemeMode } from '../lib/storage'
import type { ThemeMode } from '../types/domain'

export function useThemeMode() {
  const [themeMode, setThemeMode] = useState<ThemeMode>(() => readStoredThemeMode())

  useEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)')
    const applyTheme = () => {
      const resolvedTheme = themeMode === 'system'
        ? media.matches ? 'dark' : 'light'
        : themeMode
      document.documentElement.dataset.theme = resolvedTheme
      document.documentElement.dataset.themeMode = themeMode
      window.localStorage.setItem('llm-proxy-theme', themeMode)
    }
    applyTheme()
    media.addEventListener('change', applyTheme)
    return () => media.removeEventListener('change', applyTheme)
  }, [themeMode])

  return { themeMode, setThemeMode }
}
