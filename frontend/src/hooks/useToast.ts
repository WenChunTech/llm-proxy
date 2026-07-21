import { useEffect, useState } from 'react'

export function useToast(timeoutMs = 2200) {
  const [toast, setToast] = useState('')

  useEffect(() => {
    if (!toast) return
    const timer = window.setTimeout(() => setToast(''), timeoutMs)
    return () => window.clearTimeout(timer)
  }, [timeoutMs, toast])

  return { toast, setToast }
}
