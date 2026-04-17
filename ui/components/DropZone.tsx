export function InsertionIndicator({
  insertIndex,
  rowHeight,
}: {
  insertIndex: number | null
  rowHeight: number
}) {
  if (insertIndex === null) return null

  return (
    <div
      className="pointer-events-none absolute left-0 right-0 z-50 flex items-center"
      style={{ top: insertIndex * rowHeight }}
    >
      <div className="relative h-[2px] flex-1 bg-primary">
        <div className="absolute left-0 top-1/2 h-2 w-2 -translate-y-1/2 rounded-full bg-primary" />
        <div className="absolute right-0 top-1/2 h-2 w-2 -translate-y-1/2 rounded-full bg-primary" />
      </div>
    </div>
  )
}
