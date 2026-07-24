/** Build an authenticated dashboard WebSocket URL (token query for browsers). */
export function buildDashboardWsUrl(path: string, accessKey: string) {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  const normalized = path.startsWith('/') ? path : `/${path}`
  const url = new URL(`${protocol}//${window.location.host}${normalized}`)
  const token = accessKey.trim()
  if (token) url.searchParams.set('token', token)
  return url.toString()
}
