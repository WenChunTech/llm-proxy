export async function copyText(value: string) {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(value)
    return
  }
  const textarea = document.createElement('textarea')
  textarea.value = value
  textarea.style.position = 'fixed'
  textarea.style.opacity = '0'
  document.body.appendChild(textarea)
  textarea.select()
  const copied = document.execCommand('copy')
  document.body.removeChild(textarea)
  if (!copied) throw new Error('copy failed')
}

export async function readResponseStream(
  response: Response,
  onChunk: (value: string) => void,
) {
  if (!response.body) return await response.text()
  const reader = response.body.getReader()
  const decoder = new TextDecoder()
  let output = ''

  while (true) {
    const { value, done } = await reader.read()
    if (done) break
    output += decoder.decode(value, { stream: true })
    onChunk(output)
  }

  output += decoder.decode()
  return output
}

export function shellQuote(value: string) {
  return `'${value.replaceAll("'", "'\\''")}'`
}

export function downloadJson(filename: string, value: unknown) {
  const blob = new Blob([`${JSON.stringify(value, null, 2)}\n`], {
    type: 'application/json',
  })
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  link.href = url
  link.download = filename
  link.click()
  URL.revokeObjectURL(url)
}

export function downloadText(filename: string, content: string, mime = 'text/plain') {
  const blob = new Blob([content], { type: mime })
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  link.href = url
  link.download = filename
  link.click()
  URL.revokeObjectURL(url)
}
