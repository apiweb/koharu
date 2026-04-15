import { stat } from 'node:fs/promises'
import {
  PIPELINE_SINGLE,
  bootstrapApp,
  importAndOpenPage,
  openMenuItem,
} from './helpers/app'
import { prepareDetectAndOcr, runInpaint, runRender } from './helpers/pipeline'
import { selectors } from './helpers/selectors'
import { expect, test } from './helpers/test'

test.beforeEach(async ({ page }) => {
  await bootstrapApp(page)
})

test('exports rendered image via file menu', async ({ page }) => {
  await importAndOpenPage(page, PIPELINE_SINGLE)
  await prepareDetectAndOcr(page)
  await runInpaint(page)
  await runRender(page)

  const downloadPromise = page.waitForEvent('download')
  await openMenuItem(
    page,
    selectors.menu.fileTrigger,
    selectors.menu.fileExport,
  )
  const download = await downloadPromise

  const suggested = download.suggestedFilename()
  expect(suggested).toMatch(/_koharu\.[A-Za-z0-9]+$/)

  const downloadPath = await download.path()
  expect(downloadPath).toBeTruthy()
  if (!downloadPath) {
    throw new Error('Download path was empty')
  }

  const info = await stat(downloadPath)
  expect(info.size).toBeGreaterThan(0)
})

test('exports rendered pages as CBZ archive via native dialog', async ({
  page,
}) => {
  await importAndOpenPage(page, PIPELINE_SINGLE)
  await prepareDetectAndOcr(page)
  await runInpaint(page)
  await runRender(page)

  // Mock the archive export endpoint (native dialog is server-side)
  await page.route('**/api/v1/exports/archive', async (route) => {
    const request = route.request()
    const body = JSON.parse(request.postData() ?? '{}')
    expect(body.format).toBe('cbz')
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ count: 1 }),
    })
  })

  const requestPromise = page.waitForRequest('**/api/v1/exports/archive')
  await page.getByTestId(selectors.menu.fileTrigger).click()
  await page.getByTestId(selectors.menu.fileExportArchive).hover()
  await page.getByTestId(selectors.menu.fileExportArchiveCbz).click()
  const request = await requestPromise

  expect(request.method()).toBe('POST')
  const body = JSON.parse(request.postData() ?? '{}')
  expect(body.format).toBe('cbz')
})

test('exports rendered pages as CB7 archive via native dialog', async ({
  page,
}) => {
  await importAndOpenPage(page, PIPELINE_SINGLE)
  await prepareDetectAndOcr(page)
  await runInpaint(page)
  await runRender(page)

  // Mock the archive export endpoint (native dialog is server-side)
  await page.route('**/api/v1/exports/archive', async (route) => {
    const request = route.request()
    const body = JSON.parse(request.postData() ?? '{}')
    expect(body.format).toBe('cb7')
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ count: 1 }),
    })
  })

  const requestPromise = page.waitForRequest('**/api/v1/exports/archive')
  await page.getByTestId(selectors.menu.fileTrigger).click()
  await page.getByTestId(selectors.menu.fileExportArchive).hover()
  await page.getByTestId(selectors.menu.fileExportArchiveCb7).click()
  const request = await requestPromise

  expect(request.method()).toBe('POST')
  const body = JSON.parse(request.postData() ?? '{}')
  expect(body.format).toBe('cb7')
})
