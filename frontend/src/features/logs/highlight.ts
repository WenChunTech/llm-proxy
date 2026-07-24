export function prettyMaybeJson(content: string) {
  const trimmed = content.trim()
  if (!trimmed.startsWith('{') && !trimmed.startsWith('[')) return content
  try {
    return JSON.stringify(JSON.parse(trimmed), null, 2)
  } catch {
    return content
  }
}

export function escapeHtml(source: string) {
  return source
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
}

/** Minimal JSON highlighter — escapes HTML then wraps tokens. */
export function highlightJson(source: string) {
  const escaped = escapeHtml(source)
  return escaped.replace(
    /("(?:\\.|[^"\\])*")\s*:|"((?:\\.|[^"\\])*)"|(\btrue\b|\bfalse\b|\bnull\b)|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)|([{}[\](),:])/g,
    (match, key, str, lit, num, punct) => {
      if (key !== undefined) return `<span class="tok-key">${key}</span>:`
      if (str !== undefined) return `<span class="tok-str">"${str}"</span>`
      if (lit !== undefined) return `<span class="tok-lit">${lit}</span>`
      if (num !== undefined) return `<span class="tok-num">${num}</span>`
      if (punct !== undefined) return `<span class="tok-punct">${punct}</span>`
      return match
    },
  )
}

export function highlightSse(source: string) {
  return escapeHtml(source)
    .replace(/^(event:\s*)(.*)$/gm, '<span class="tok-key">$1</span><span class="tok-str">$2</span>')
    .replace(/^(data:\s*)(.*)$/gm, (_, prefix, rest) => {
      const unescaped = rest
        .replaceAll('&lt;', '<')
        .replaceAll('&gt;', '>')
        .replaceAll('&amp;', '&')
      const pretty = prettyMaybeJson(unescaped)
      if (pretty !== unescaped && (unescaped.trim().startsWith('{') || unescaped.trim().startsWith('['))) {
        return `<span class="tok-key">${prefix}</span>${highlightJson(pretty)}`
      }
      return `<span class="tok-key">${prefix}</span><span class="tok-str">${rest}</span>`
    })
    .replace(/^(id:\s*)(.*)$/gm, '<span class="tok-key">$1</span><span class="tok-num">$2</span>')
}

export function renderHighlighted(content: string, language: string) {
  if (language === 'json' || content.trim().startsWith('{') || content.trim().startsWith('[')) {
    return highlightJson(prettyMaybeJson(content))
  }
  if (language === 'sse') return highlightSse(content)
  return escapeHtml(content)
}

/** Highlight keyword matches inside already-escaped/highlighted HTML (text nodes only). */
export function highlightKeywordInHtml(html: string, keyword: string) {
  const query = keyword.trim()
  if (!query) return html
  const escapedQuery = query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const regex = new RegExp(escapedQuery, 'gi')
  let result = ''
  let i = 0
  while (i < html.length) {
    if (html[i] === '<') {
      const end = html.indexOf('>', i)
      if (end === -1) {
        result += html.slice(i)
        break
      }
      result += html.slice(i, end + 1)
      i = end + 1
      continue
    }
    const nextTag = html.indexOf('<', i)
    const chunkEnd = nextTag === -1 ? html.length : nextTag
    const text = html.slice(i, chunkEnd)
    result += text.replace(regex, (match) => `<mark class="log-hit">${match}</mark>`)
    i = chunkEnd
  }
  return result
}

