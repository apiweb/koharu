import { fetchApi } from '../fetch'
import type { ImportResult } from '../schemas'

export type ImportFromPathsParams = {
  mode?: 'replace' | 'append'
}

export type ImportFromPathsBody = {
  paths: string[]
  insertAt?: number
}

export const importFromPaths = async (
  body: ImportFromPathsBody,
  params?: ImportFromPathsParams,
  options?: RequestInit,
): Promise<ImportResult> => {
  const normalizedParams = new URLSearchParams()
  if (params?.mode) {
    normalizedParams.append('mode', params.mode)
  }

  const stringifiedParams = normalizedParams.toString()
  const url =
    stringifiedParams.length > 0
      ? `/api/v1/documents/import-paths?${stringifiedParams}`
      : `/api/v1/documents/import-paths`

  return fetchApi<ImportResult>(url, {
    ...options,
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...options?.headers,
    },
    body: JSON.stringify(body),
  })
}
