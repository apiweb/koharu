import path from 'node:path'
import {
  bootstrapApp,
  importImages,
  openNavigatorPage,
  waitForNavigatorPageCount,
  waitForWorkspaceImage,
} from './helpers/app'
import { selectors } from './helpers/selectors'
import { test } from './helpers/test'

const FIXTURES_DIR = path.join(process.cwd(), 'e2e', 'fixtures')
const CBZ_FIXTURE = path.join(FIXTURES_DIR, 'cbz_test_file.cbz')
const CB7_FIXTURE = path.join(FIXTURES_DIR, 'cb7_test_file.cb7')
const CBZ_PAGE_COUNT = 3
const CB7_PAGE_COUNT = 3

const STANDALONE_IMAGES = [
  path.join(FIXTURES_DIR, '19.jpg'),
  path.join(FIXTURES_DIR, '21.jpg'),
]

test.beforeEach(async ({ page }) => {
  await bootstrapApp(page)
})

test('imports a CBZ archive and shows all extracted pages', async ({
  page,
}) => {
  await importImages(page, [CBZ_FIXTURE])
  await waitForNavigatorPageCount(page, CBZ_PAGE_COUNT)
})

test('imports a CB7 archive and shows all extracted pages', async ({
  page,
}) => {
  await importImages(page, [CB7_FIXTURE])
  await waitForNavigatorPageCount(page, CB7_PAGE_COUNT)
})

test('navigates through CBZ-extracted pages in workspace', async ({ page }) => {
  await importImages(page, [CBZ_FIXTURE])
  await waitForNavigatorPageCount(page, CBZ_PAGE_COUNT)

  await openNavigatorPage(page, 0)
  await waitForWorkspaceImage(page)

  await openNavigatorPage(page, CBZ_PAGE_COUNT - 1)
  await waitForWorkspaceImage(page)
})

test('navigates through CB7-extracted pages in workspace', async ({ page }) => {
  await importImages(page, [CB7_FIXTURE])
  await waitForNavigatorPageCount(page, CB7_PAGE_COUNT)

  await openNavigatorPage(page, 0)
  await waitForWorkspaceImage(page)

  await openNavigatorPage(page, CB7_PAGE_COUNT - 1)
  await waitForWorkspaceImage(page)
})

test('appends CBZ pages to existing images', async ({ page }) => {
  await importImages(page, STANDALONE_IMAGES)
  await waitForNavigatorPageCount(page, STANDALONE_IMAGES.length)
  await openNavigatorPage(page, 0)
  await waitForWorkspaceImage(page)

  const fileChooserPromise = page.waitForEvent('filechooser')
  await page.getByTestId(selectors.menu.fileTrigger).click()
  await page.getByTestId('menu-file-add').click()
  const fileChooser = await fileChooserPromise
  await fileChooser.setFiles([CBZ_FIXTURE])

  await waitForNavigatorPageCount(
    page,
    STANDALONE_IMAGES.length + CBZ_PAGE_COUNT,
  )
})

test('appends CB7 pages to existing images', async ({ page }) => {
  await importImages(page, STANDALONE_IMAGES)
  await waitForNavigatorPageCount(page, STANDALONE_IMAGES.length)
  await openNavigatorPage(page, 0)
  await waitForWorkspaceImage(page)

  const fileChooserPromise = page.waitForEvent('filechooser')
  await page.getByTestId(selectors.menu.fileTrigger).click()
  await page.getByTestId('menu-file-add').click()
  const fileChooser = await fileChooserPromise
  await fileChooser.setFiles([CB7_FIXTURE])

  await waitForNavigatorPageCount(
    page,
    STANDALONE_IMAGES.length + CB7_PAGE_COUNT,
  )
})

test('imports mixed loose images and archives together', async ({ page }) => {
  await importImages(page, [...STANDALONE_IMAGES, CBZ_FIXTURE])
  await waitForNavigatorPageCount(
    page,
    STANDALONE_IMAGES.length + CBZ_PAGE_COUNT,
  )

  await openNavigatorPage(page, 0)
  await waitForWorkspaceImage(page)

  const lastIndex = STANDALONE_IMAGES.length + CBZ_PAGE_COUNT - 1
  await openNavigatorPage(page, lastIndex)
  await waitForWorkspaceImage(page)
})
