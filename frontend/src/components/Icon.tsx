const iconPaths: Record<string, string> = {
  server:
    'M4 5.5A1.5 1.5 0 0 1 5.5 4h13A1.5 1.5 0 0 1 20 5.5v3A1.5 1.5 0 0 1 18.5 10h-13A1.5 1.5 0 0 1 4 8.5zM4 15.5A1.5 1.5 0 0 1 5.5 14h13a1.5 1.5 0 0 1 1.5 1.5v3a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 18.5zM7 7h.01M7 17h.01',
  route:
    'M5 5h5M14 5h5M5 19h5M14 19h5M10 5l4 14M14 5l-4 14',
  pulse: 'M3 12h4l2-7 4 14 2-7h6',
  plus: 'M12 5v14M5 12h14',
  search: 'm20 20-4.4-4.4M10.8 17a6.2 6.2 0 1 1 0-12.4 6.2 6.2 0 0 1 0 12.4Z',
  edit: 'M4 20h4l10.5-10.5a2.12 2.12 0 0 0-3-3L5 17v3ZM14.5 7.5l3 3',
  trash: 'M5 7h14M10 11v5M14 11v5M7 7l1 13h8l1-13M9 7V4h6v3',
  copy: 'M8 8h9a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-9a2 2 0 0 1-2-2v-9a2 2 0 0 1 2-2ZM5 16H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1',
  download: 'M12 3v12M7 10l5 5 5-5M5 21h14',
  upload: 'M12 21V9M7 14l5-5 5 5M5 3h14',
  play: 'M8 5v14l11-7Z',
  sun: 'M12 4V2M12 22v-2M4.93 4.93 3.52 3.52M20.48 20.48l-1.41-1.41M4 12H2M22 12h-2M4.93 19.07l-1.41 1.41M20.48 3.52l-1.41 1.41M12 17a5 5 0 1 0 0-10 5 5 0 0 0 0 10Z',
  moon: 'M21 14.5A8.5 8.5 0 0 1 9.5 3 7 7 0 1 0 21 14.5Z',
  monitor: 'M4 5h16v10H4zM9 21h6M12 15v6',
  key: 'M15 7a4 4 0 1 0 2.8 6.8L21 17v3h-3v-2h-2v-2h-2l-1.8-1.8A4 4 0 0 0 15 7ZM7 11h.01',
  check: 'm5 12 4 4L19 6',
  close: 'M6 6l12 12M18 6 6 18',
  arrowUp: 'm18 15-6-6-6 6',
  arrowDown: 'm6 9 6 6 6-6',
  toTop: 'M5 5h14M12 19V9M8 13l4-4 4 4',
  toBottom: 'M5 19h14M12 5v10M8 11l4 4 4-4',
  chevron: 'm9 18 6-6-6-6',
  external: 'M14 5h5v5M19 5l-8 8',
  grip: 'M8 6h.01M8 12h.01M8 18h.01M16 6h.01M16 12h.01M16 18h.01',
  sidebar: 'M4 5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2zM9 3v18',
  terminal: 'M4 5h16v14H4zM8 9l3 3-3 3M12 15h4',
}

export function Icon({ name, size = 18 }: { name: string; size?: number }) {
  return (
    <svg
      className="icon"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d={iconPaths[name]} />
    </svg>
  )
}
