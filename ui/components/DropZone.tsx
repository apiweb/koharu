'use client'

// ---------------------------------------------------------------------------
// InsertionIndicator
// ---------------------------------------------------------------------------

interface InsertionIndicatorProps {
  top: number
}
export function InsertionIndicator({ top }: InsertionIndicatorProps) {
  return (
    <div
      data-testid='drop-insertion-indicator'
      className='pointer-events-none absolute left-1 right-1 z-10 flex items-center'
      style={{ top }}
    >
      {/* Left dot */}
      <div className='bg-primary h-2 w-2 shrink-0 rounded-full' />
      {/* Line */}
      <div className='bg-primary h-0.5 flex-1' />
      {/* Right dot */}
      <div className='bg-primary h-2 w-2 shrink-0 rounded-full' />
    </div>
  )
}
