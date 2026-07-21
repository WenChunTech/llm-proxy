export function moveItem<T>(items: T[], index: number, direction: -1 | 1): T[] | null {
  const targetIndex = index + direction
  if (index < 0 || targetIndex < 0 || index >= items.length || targetIndex >= items.length) {
    return null
  }
  const next = [...items]
  const [moved] = next.splice(index, 1)
  next.splice(targetIndex, 0, moved)
  return next
}

export function reorderItem<T>(items: T[], sourceIndex: number, targetIndex: number): T[] | null {
  if (
    sourceIndex === targetIndex ||
    sourceIndex < 0 ||
    targetIndex < 0 ||
    sourceIndex >= items.length ||
    targetIndex >= items.length
  ) {
    return null
  }
  const next = [...items]
  const [moved] = next.splice(sourceIndex, 1)
  next.splice(targetIndex, 0, moved)
  return next
}

export function reorderSameKindProviders(
  providers: Array<{ id: string; kind: string }>,
  sourceId: string,
  targetId: string,
) {
  if (!sourceId || !targetId || sourceId === targetId) return null
  const source = providers.find((item) => item.id === sourceId)
  const target = providers.find((item) => item.id === targetId)
  if (!source || !target || source.kind !== target.kind) return null

  const kindProviders = providers.filter((item) => item.kind === source.kind)
  const sourceIndex = kindProviders.findIndex((item) => item.id === sourceId)
  const targetIndex = kindProviders.findIndex((item) => item.id === targetId)
  const reordered = reorderItem(kindProviders, sourceIndex, targetIndex)
  if (!reordered) return null

  let cursor = 0
  return providers.map((item) => (item.kind === source.kind ? reordered[cursor++] : item))
}
