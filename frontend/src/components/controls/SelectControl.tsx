import { useEffect, useRef, useState } from 'react'
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
  ariaLabel,
}: {
  value: T
  options: SelectOption<T>[]
  onChange: (value: T) => void
  disabled?: boolean
  compact?: boolean
  mono?: boolean
  ariaLabel: string
}) {
  const rootRef = useRef<HTMLDivElement | null>(null)
  const [open, setOpen] = useState(false)
  const selectedIndex = Math.max(0, options.findIndex((option) => option.value === value))
  const [highlightedIndex, setHighlightedIndex] = useState(selectedIndex)
  const selected = options[selectedIndex]

  useEffect(() => {
    if (!open) return
    setHighlightedIndex(selectedIndex)
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false)
    }
    document.addEventListener('pointerdown', closeOnOutsidePointer)
    return () => document.removeEventListener('pointerdown', closeOnOutsidePointer)
  }, [open, selectedIndex])

  function commit(option: SelectOption<T>) {
    if (disabled) return
    onChange(option.value)
    setOpen(false)
  }

  function moveHighlight(direction: -1 | 1) {
    if (!options.length) return
    setHighlightedIndex((index) => (index + direction + options.length) % options.length)
  }

  return (
    <div
      className={`select-control ${compact ? 'compact-select' : ''} ${mono ? 'mono-select' : ''} ${open ? 'open' : ''} ${disabled ? 'disabled' : ''}`}
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
        onKeyDown={(event) => {
          if (event.key === 'ArrowDown') {
            event.preventDefault()
            if (!open) setOpen(true)
            moveHighlight(1)
          } else if (event.key === 'ArrowUp') {
            event.preventDefault()
            if (!open) setOpen(true)
            moveHighlight(-1)
          } else if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault()
            if (open && options[highlightedIndex]) {
              commit(options[highlightedIndex])
            } else {
              setOpen(true)
            }
          } else if (event.key === 'Escape') {
            setOpen(false)
          }
        }}
      >
        {selected?.accent && <span className="select-accent" style={{ backgroundColor: selected.accent }} />}
        <span className="select-value" title={selected?.label ?? ''}>{selected?.label ?? ''}</span>
        <Icon name="chevron" size={13} />
      </button>
      {open && !disabled && (
        <div className="select-menu" role="listbox" aria-label={ariaLabel}>
          {options.map((option, index) => (
            <button
              className={`select-option ${option.value === value ? 'selected' : ''} ${index === highlightedIndex ? 'highlighted' : ''}`}
              type="button"
              role="option"
              aria-selected={option.value === value}
              key={option.value || option.label}
              onMouseEnter={() => setHighlightedIndex(index)}
              onClick={() => commit(option)}
            >
              {option.accent && <span className="select-accent" style={{ backgroundColor: option.accent }} />}
              <span title={option.label}>{option.label}</span>
              {option.value === value && <Icon name="check" size={13} />}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
