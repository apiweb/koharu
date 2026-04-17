import path from 'node:path'
import {
  PIPELINE_SINGLE,
  SMOKE_SET,
  bootstrapApp,
  importImages,
  waitForNavigatorPageCount,
} from './helpers/app'
import { expect, test } from './helpers/test'

const API_BASE = 'http://127.0.0.1:9999/api/v1'
const FIXTURES_DIR = path.join(process.cwd(), 'e2e', 'fixtures')

test.beforeEach(async ({ page }) => {
  await page.request.post(`${API_BASE}/documents/import-paths`, {
    data: { paths: [path.join(FIXTURES_DIR, '1.jpg')] },
  })
  await bootstrapApp(page)
})

test('import-paths appends images via API', async ({ page }) => {
  await importImages(page, PIPELINE_SINGLE)
  await waitForNavigatorPageCount(page, 1)

  const paths = [
    path.join(FIXTURES_DIR, '10.jpg'),
    path.join(FIXTURES_DIR, '11.jpg'),
  ]

  const response = await page.request.post(
    `${API_BASE}/documents/import-paths`,
    { data: { paths } },
  )
  expect(response.ok()).toBe(true)

  const body = await response.json()
  expect(body.totalCount).toBe(3)

  await page.reload()
  await waitForNavigatorPageCount(page, 3)
})

test('import-paths inserts at specific position', async ({ page }) => {
  const initial = SMOKE_SET.slice(0, 3)
  await importImages(page, initial)
  await waitForNavigatorPageCount(page, 3)

  const paths = [path.join(FIXTURES_DIR, '19.jpg')]

  const response = await page.request.post(
    `${API_BASE}/documents/import-paths`,
    { data: { paths, insertAt: 1 } },
  )
  expect(response.ok()).toBe(true)

  const body = await response.json()
  expect(body.totalCount).toBe(4)

  await page.reload()
  await waitForNavigatorPageCount(page, 4)
})

test('import-paths skips duplicate images', async ({ page }) => {
  await importImages(page, PIPELINE_SINGLE)
  await waitForNavigatorPageCount(page, 1)

  const response = await page.request.post(
    `${API_BASE}/documents/import-paths`,
    { data: { paths: [PIPELINE_SINGLE[0]] } },
  )

  // Dedup response is 200 but no new pages are added.
  expect(response.ok()).toBe(true)
  const body = await response.json()
  expect(body.totalCount).toBe(1)

  await waitForNavigatorPageCount(page, 1)
})

test('import-paths rejects non-image files', async ({ page }) => {
  const response = await page.request.post(
    `${API_BASE}/documents/import-paths`,
    { data: { paths: ['/tmp/not-an-image.txt'] } },
  )
  expect(response.ok()).toBe(false)
})

test('import-paths rejects empty paths array', async ({ page }) => {
  const response = await page.request.post(
    `${API_BASE}/documents/import-paths`,
    { data: { paths: [] } },
  )
  expect(response.ok()).toBe(false)
})
