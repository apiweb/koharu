import type { ImportPathsRequest } from '../schemas/importPathsRequest'
import type { ImportResult } from '../schemas/importResult'

export async function importFromPaths(
  body: ImportPathsRequest,
  options?: RequestInit,
): Promise<ImportResult> {
  const url = '/api/v1/documents/import-paths'
  const response = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
    ...options,
  })
  if (!response.ok) {
    const text = await response.text()
    throw new Error(`HTTP ${response.status}: ${text}`)
  }
  return response.json()
}
