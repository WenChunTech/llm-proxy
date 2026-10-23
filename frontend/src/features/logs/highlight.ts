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

/** Terminal palette (dark console) for 3/4-bit SGR colors: 30-37 and 90-97. */
const ANSI_PALETTE = [
  '#666666', '#f14c4c', '#23d18b', '#f5f543',
  '#3b8eea', '#d670d6', '#29b8db', '#e5e5e5',
  '#8b949e', '#ff7b72', '#7ee787', '#e3b341',
  '#79c0ff', '#d2a8ff', '#76e3ea', '#ffffff',
]

function ansi256Color(code: number) {
  if (code < 16) return ANSI_PALETTE[code] ?? undefined
  if (code < 232) {
    const n = code - 16
    const step = (value: number) => Math.round(value * 255)
    const levels = [0, 0.2, 0.3137, 0.4275, 0.5412, 0.6549, 0.7686, 0.8823, 1]
    const r = step(levels[Math.floor(n / 36)])
    const g = step(levels[Math.floor((n % 36) / 6)])
    const b = step(levels[n % 6])
    return `rgb(${r}, ${g}, ${b})`
  }
  const gray = Math.round((8 + (code - 232) * 10) * 2.55)
  return `rgb(${gray}, ${gray}, ${gray})`
}

type AnsiStyle = {
  fg?: string
  bg?: string
  bold?: boolean
  dim?: boolean
  italic?: boolean
  underline?: boolean
}

function applySgr(params: string, style: AnsiStyle) {
  const codes = params.split(';').map((part) => Number.parseInt(part || '0', 10))
  let index = 0
  while (index < codes.length) {
    const code = codes[index]
    if (code === 0) {
      for (const key of Object.keys(style) as Array<keyof AnsiStyle>) delete style[key]
    } else if (code === 1) {
      style.bold = true
    } else if (code === 2) {
      style.dim = true
    } else if (code === 3) {
      style.italic = true
    } else if (code === 4) {
      style.underline = true
    } else if (code === 22) {
      style.bold = false
      style.dim = false
    } else if (code === 23) {
      style.italic = false
    } else if (code === 24) {
      style.underline = false
    } else if (code === 39) {
      delete style.fg
    } else if (code === 49) {
      delete style.bg
    } else if ((code >= 30 && code <= 37) || (code >= 90 && code <= 97)) {
      style.fg = code >= 90 ? ANSI_PALETTE[code - 90 + 8] : ANSI_PALETTE[code - 30]
    } else if ((code >= 40 && code <= 47) || (code >= 100 && code <= 107)) {
      style.bg = code >= 100 ? ANSI_PALETTE[code - 100 + 8] : ANSI_PALETTE[code - 40]
    } else if (code === 38 || code === 48) {
      const mode = codes[index + 1]
      let color: string | undefined
      if (mode === 5) {
        color = ansi256Color(codes[index + 2])
        index += 2
      } else if (mode === 2) {
        const toHex = (value: number) => Math.min(255, Math.max(0, value)).toString(16).padStart(2, '0')
        color = `#${toHex(codes[index + 2])}${toHex(codes[index + 3])}${toHex(codes[index + 4])}`
        index += 4
      }
      if (color) {
        if (code === 38) style.fg = color
        else style.bg = color
      }
    }
    index += 1
  }
}

function ansiSpanClass(style: AnsiStyle) {
  return [
    style.bold ? 'ansi-bold' : '',
    style.dim ? 'ansi-dim' : '',
    style.italic ? 'ansi-italic' : '',
    style.underline ? 'ansi-underline' : '',
  ]
    .filter(Boolean)
    .join(' ')
}

function ansiSpanHtml(escapedText: string, style: AnsiStyle) {
  const cls = ansiSpanClass(style)
  const styleAttr = `${style.fg ? `color:${style.fg};` : ''}${style.bg ? `background-color:${style.bg};` : ''}`
  if (!cls && !styleAttr) return escapedText
  return `<span${cls ? ` class="${cls}"` : ''}${styleAttr ? ` style="${styleAttr}"` : ''}>${escapedText}</span>`
}

/**
 * Render text with ANSI SGR escape codes (terminal colors emitted by the
 * tracing subscriber) as HTML. Plain text without escape codes passes through
 * HTML-escaped unchanged.
 */
export function ansiToHtml(text: string) {
  let out = ''
  let cursor = 0
  const style: AnsiStyle = {}
  while (cursor < text.length) {
    const esc = text.indexOf('\u001b', cursor)
    const chunkEnd = esc === -1 ? text.length : esc
    if (chunkEnd > cursor) {
      out += ansiSpanHtml(escapeHtml(text.slice(cursor, chunkEnd)), style)
    }
    if (esc === -1) break
    if (text[esc + 1] === '[') {
      let end = esc + 2
      while (end < text.length) {
        const code = text.charCodeAt(end)
        if (code >= 0x40 && code <= 0x7e) break
        end += 1
      }
      const finalByte = text[end]
      const params = text.slice(esc + 2, end)
      if (finalByte === 'm') applySgr(params, style)
      cursor = end + 1
    } else {
      // Non-CSI escape (OSC etc.): skip the introducer byte.
      cursor = esc + 2
    }
  }
  return out
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
      // Keep file-saved formatting: never pretty-print streaming JSON payloads.
      const unescaped = rest
        .replaceAll('&lt;', '<')
        .replaceAll('&gt;', '>')
        .replaceAll('&amp;', '&')
      const trimmed = unescaped.trim()
      if (trimmed.startsWith('{') || trimmed.startsWith('[')) {
        try {
          JSON.parse(trimmed)
          return `<span class="tok-key">${prefix}</span>${highlightJson(unescaped)}`
        } catch {
          // fall through to plain text
        }
      }
      return `<span class="tok-key">${prefix}</span><span class="tok-str">${rest}</span>`
    })
    .replace(/^(id:\s*)(.*)$/gm, '<span class="tok-key">$1</span><span class="tok-num">$2</span>')
}

export function renderHighlighted(content: string, language: string) {
  if (language === 'sse') return highlightSse(content)
  if (language === 'json' || content.trim().startsWith('{') || content.trim().startsWith('[')) {
    return highlightJson(prettyMaybeJson(content))
  }
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

