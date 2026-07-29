import { useEffect, useMemo, useRef, useState } from 'react'
import { Icon } from '../Icon'

export type SelectOption<T extends string> = {
  value: T
  label: string
  accent?: string
}

export function SelectControl<T extends string>({
  value,
  options,
  onChange,
  disabled = false,
  compact = false,
  mono = false,
  searchable = false,
  searchPlaceholder = '筛选…',
  ariaLabel,
}: {
  value: T
  options: SelectOption<T>[]
  onChange: (value: T) => void
  disabled?: boolean
  compact?: boolean
  mono?: boolean
  searchable?: boolean
  searchPlaceholder?: string
  ariaLabel: string
}) {
  const rootRef = useRef<HTMLDivElement | null>(null)
  const searchRef = useRef<HTMLInputElement | null>(null)
  const wasOpenRef = useRef(false)
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const selectedIndex = Math.max(0, options.findIndex((option) => option.value === value))
  const [highlightedIndex, setHighlightedIndex] = useState(selectedIndex)
  const selected = options[selectedIndex]

  const filteredOptions = useMemo(() => {
    if (!searchable) return options
    const normalized = query.trim().toLowerCase()
    if (!normalized) return options
    return options.filter((option) => {
      if (!option.value) return false
      return (
        option.label.toLowerCase().includes(normalized) ||
        option.value.toLowerCase().includes(normalized)
      )
    })
  }, [options, query, searchable])

  useEffect(() => {
    if (!open) {
      setQuery('')
      wasOpenRef.current = false
      return
    }

    if (!wasOpenRef.current) {
      const matched = filteredOptions.findIndex((option) => option.value === value)
      setHighlightedIndex(matched >= 0 ? matched : 0)
      wasOpenRef.current = true
    }

    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false)
    }
    document.addEventListener('pointerdown', closeOnOutsidePointer)
    return () => document.removeEventListener('pointerdown', closeOnOutsidePointer)
  }, [open, filteredOptions, value])

  useEffect(() => {
    if (!open || !searchable) return
    const frame = window.requestAnimationFrame(() => {
      searchRef.current?.focus()
    })
    return () => window.cancelAnimationFrame(frame)
  }, [open, searchable])

  function commit(option: SelectOption<T>) {
    if (disabled) return
    onChange(option.value)
    setOpen(false)
    setQuery('')
  }

  function moveHighlight(direction: -1 | 1) {
    if (!filteredOptions.length) return
    setHighlightedIndex((index) => (index + direction + filteredOptions.length) % filteredOptions.length)
  }

  function handleTriggerKeyDown(event: React.KeyboardEvent<HTMLButtonElement>) {
    if (event.key === 'ArrowDown') {
      event.preventDefault()
      if (!open) setOpen(true)
      else moveHighlight(1)
    } else if (event.key === 'ArrowUp') {
      event.preventDefault()
      if (!open) setOpen(true)
      else moveHighlight(-1)
    } else if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault()
      if (open && filteredOptions[highlightedIndex]) {
        commit(filteredOptions[highlightedIndex])
      } else {
        setOpen(true)
      }
    } else if (event.key === 'Escape') {
      setOpen(false)
      setQuery('')
    } else if (
      searchable &&
      !event.metaKey &&
      !event.ctrlKey &&
      !event.altKey &&
      event.key.length === 1
    ) {
      setOpen(true)
      setQuery(event.key)
      setHighlightedIndex(0)
    }
  }

  function handleSearchKeyDown(event: React.KeyboardEvent<HTMLInputElement>) {
    if (event.key === 'ArrowDown') {
      event.preventDefault()
      moveHighlight(1)
    } else if (event.key === 'ArrowUp') {
      event.preventDefault()
      moveHighlight(-1)
    } else if (event.key === 'Enter') {
      event.preventDefault()
      if (filteredOptions[highlightedIndex]) commit(filteredOptions[highlightedIndex])
    } else if (event.key === 'Escape') {
      event.preventDefault()
      if (query) {
        setQuery('')
        setHighlightedIndex(0)
      } else {
        setOpen(false)
      }
    }
  }

  return (
    <div
      className={`select-control ${compact ? 'compact-select' : ''} ${mono ? 'mono-select' : ''} ${open ? 'open' : ''} ${disabled ? 'disabled' : ''} ${searchable ? 'searchable-select' : ''}`}
      ref={rootRef}
    >
      <button
        className="select-trigger"
        type="button"
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        disabled={disabled}
        onClick={() => setOpen((current) => !current)}
        onKeyDown={handleTriggerKeyDown}
      >
        {selected?.accent && <span className="select-accent" style={{ backgroundColor: selected.accent }} />}
        <span className="select-value" title={selected?.label ?? ''}>{selected?.label ?? ''}</span>
        <Icon name="chevron" size={13} />
      </button>
      {open && !disabled && (
        <div className="select-menu" role="listbox" aria-label={ariaLabel}>
          {searchable && (
            <div className="select-search" onPointerDown={(event) => event.stopPropagation()}>
              <Icon name="search" size={14} />
              <input
                ref={searchRef}
                type="search"
                value={query}
                placeholder={searchPlaceholder}
                aria-label={`${ariaLabel}筛选`}
                autoComplete="off"
                spellCheck={false}
                onChange={(event) => {
                  setQuery(event.target.value)
                  setHighlightedIndex(0)
                }}
                onKeyDown={handleSearchKeyDown}
              />
            </div>
          )}
          {filteredOptions.map((option, index) => (
            <button
              className={`select-option ${option.value === value ? 'selected' : ''} ${index === highlightedIndex ? 'highlighted' : ''}`}
              type="button"
              role="option"
              aria-selected={option.value === value}
              key={`${option.value || option.label}:${index}`}
              onMouseEnter={() => setHighlightedIndex(index)}
              onClick={() => commit(option)}
            >
              {option.accent && <span className="select-accent" style={{ backgroundColor: option.accent }} />}
              <span title={option.label}>{option.label}</span>
              {option.value === value && <Icon name="check" size={13} />}
            </button>
          ))}
          {!filteredOptions.length && (
            <div className="select-empty">无匹配项</div>
          )}
        </div>
      )}
    </div>
  )
}
