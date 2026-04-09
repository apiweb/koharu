import { PIPELINE_SINGLE, bootstrapApp, importAndOpenPage } from './helpers/app'
import { prepareDetectAndOcr } from './helpers/pipeline'
import { selectors } from './helpers/selectors'
import { expect, test } from './helpers/test'

test.beforeEach(async ({ page }) => {
  await bootstrapApp(page)
})

test('copy and paste buttons are disabled when no block is selected', async ({
  page,
}) => {
  await importAndOpenPage(page, PIPELINE_SINGLE)

  await page.getByTestId(selectors.panels.tabLayout).click()
  await expect(page.getByTestId(selectors.panels.layout)).toBeVisible()

  const copyBtn = page.getByTestId(selectors.panels.renderCopyStyle)
  const pasteBtn = page.getByTestId(selectors.panels.renderPasteStyle)
  await expect(copyBtn).toBeDisabled()
  await expect(pasteBtn).toBeDisabled()
})

test('copy enables paste button on another block', async ({ page }) => {
  await importAndOpenPage(page, PIPELINE_SINGLE)
  await prepareDetectAndOcr(page)

  await page.getByTestId(selectors.panels.tabLayout).click()
  await expect(page.getByTestId(selectors.panels.layout)).toBeVisible()

  const copyBtn = page.getByTestId(selectors.panels.renderCopyStyle)
  const pasteBtn = page.getByTestId(selectors.panels.renderPasteStyle)
  const scopeIndicator = page.getByTestId(selectors.panels.renderScopeIndicator)

  await expect(scopeIndicator).toHaveText(/Block 1/)
  await expect(copyBtn).toBeEnabled()
  await expect(pasteBtn).toBeDisabled()

  await copyBtn.click()

  await page.getByTestId(selectors.panels.textBlockCard(1)).click()
  await expect(scopeIndicator).toHaveText(/Block 2/)
  await expect(pasteBtn).toBeEnabled()
})

test('paste applies copied style to target block', async ({ page }) => {
  await importAndOpenPage(page, PIPELINE_SINGLE)
  await prepareDetectAndOcr(page)

  await page.getByTestId(selectors.panels.tabLayout).click()
  await expect(page.getByTestId(selectors.panels.layout)).toBeVisible()

  const copyBtn = page.getByTestId(selectors.panels.renderCopyStyle)
  const pasteBtn = page.getByTestId(selectors.panels.renderPasteStyle)
  const swatch = page.getByTestId(selectors.panels.renderColorSwatch)
  const scopeIndicator = page.getByTestId(selectors.panels.renderScopeIndicator)

  await expect(scopeIndicator).toHaveText(/Block 1/)
  const sourceColor = await swatch.evaluate((node) => {
    if (!(node instanceof HTMLElement)) return ''
    return node.style.backgroundColor
  })
  expect(sourceColor).not.toBe('')

  await copyBtn.click()

  await page.getByTestId(selectors.panels.textBlockCard(1)).click()
  await expect(scopeIndicator).toHaveText(/Block 2/)

  await pasteBtn.click()

  await expect
    .poll(
      async () =>
        swatch.evaluate((node) => {
          if (!(node instanceof HTMLElement)) return ''
          return node.style.backgroundColor
        }),
      { timeout: 10_000 },
    )
    .toBe(sourceColor)
})

test('paste in global scope applies style to all blocks', async ({ page }) => {
  await importAndOpenPage(page, PIPELINE_SINGLE)
  await prepareDetectAndOcr(page)

  await page.getByTestId(selectors.panels.tabLayout).click()
  await expect(page.getByTestId(selectors.panels.layout)).toBeVisible()

  const copyBtn = page.getByTestId(selectors.panels.renderCopyStyle)
  const pasteBtn = page.getByTestId(selectors.panels.renderPasteStyle)
  const swatch = page.getByTestId(selectors.panels.renderColorSwatch)
  const scopeIndicator = page.getByTestId(selectors.panels.renderScopeIndicator)

  await expect(scopeIndicator).toHaveText(/Block 1/)
  const sourceColor = await swatch.evaluate((node) => {
    if (!(node instanceof HTMLElement)) return ''
    return node.style.backgroundColor
  })

  await copyBtn.click()

  await page
    .getByTestId(selectors.panels.textBlockCard(0))
    .getByRole('heading')
    .click()
  await expect(scopeIndicator).toHaveText(/Global/)

  await expect(pasteBtn).toBeEnabled()
  await pasteBtn.click()

  await page.getByTestId(selectors.panels.textBlockCard(1)).click()
  await expect(scopeIndicator).toHaveText(/Block 2/)

  await expect
    .poll(
      async () =>
        swatch.evaluate((node) => {
          if (!(node instanceof HTMLElement)) return ''
          return node.style.backgroundColor
        }),
      { timeout: 10_000 },
    )
    .toBe(sourceColor)
})

test('keyboard shortcut copies and pastes formatting', async ({ page }) => {
  await importAndOpenPage(page, PIPELINE_SINGLE)
  await prepareDetectAndOcr(page)

  await page.getByTestId(selectors.panels.tabLayout).click()
  await expect(page.getByTestId(selectors.panels.layout)).toBeVisible()

  const swatch = page.getByTestId(selectors.panels.renderColorSwatch)
  const scopeIndicator = page.getByTestId(selectors.panels.renderScopeIndicator)
  const pasteBtn = page.getByTestId(selectors.panels.renderPasteStyle)

  await expect(scopeIndicator).toHaveText(/Block 1/)
  const sourceColor = await swatch.evaluate((node) => {
    if (!(node instanceof HTMLElement)) return ''
    return node.style.backgroundColor
  })

  const isMac = await page.evaluate(() => navigator.platform.includes('Mac'))
  const copyMod = isMac ? 'Meta' : 'Control'
  await page.keyboard.press(`${copyMod}+Alt+c`)

  await page.getByTestId(selectors.panels.textBlockCard(1)).click()
  await expect(scopeIndicator).toHaveText(/Block 2/)
  await expect(pasteBtn).toBeEnabled()

  await page.keyboard.press(`${copyMod}+Alt+v`)

  await expect
    .poll(
      async () =>
        swatch.evaluate((node) => {
          if (!(node instanceof HTMLElement)) return ''
          return node.style.backgroundColor
        }),
      { timeout: 10_000 },
    )
    .toBe(sourceColor)
})

test('copy shows check icon feedback', async ({ page }) => {
  await importAndOpenPage(page, PIPELINE_SINGLE)
  await prepareDetectAndOcr(page)

  await page.getByTestId(selectors.panels.tabLayout).click()
  await expect(page.getByTestId(selectors.panels.layout)).toBeVisible()

  const copyBtn = page.getByTestId(selectors.panels.renderCopyStyle)
  const scopeIndicator = page.getByTestId(selectors.panels.renderScopeIndicator)

  await expect(scopeIndicator).toHaveText(/Block 1/)
  await expect(copyBtn).not.toHaveAttribute('data-copied')

  await copyBtn.click()

  await expect(copyBtn).toHaveAttribute('data-copied', 'true')
  await expect(copyBtn).not.toHaveAttribute('data-copied', { timeout: 3000 })
})

test('paste preserves all formatting properties', async ({ page }) => {
  await importAndOpenPage(page, PIPELINE_SINGLE)
  await prepareDetectAndOcr(page)

  await page.getByTestId(selectors.panels.tabLayout).click()
  await expect(page.getByTestId(selectors.panels.layout)).toBeVisible()

  const copyBtn = page.getByTestId(selectors.panels.renderCopyStyle)
  const pasteBtn = page.getByTestId(selectors.panels.renderPasteStyle)
  const scopeIndicator = page.getByTestId(selectors.panels.renderScopeIndicator)

  await expect(scopeIndicator).toHaveText(/Block 1/)

  const boldToggle = page.getByTestId('render-effect-toggle-bold')
  await boldToggle.click()

  const alignRight = page.getByTestId('render-align-right')
  await alignRight.click()

  const swatch = page.getByTestId(selectors.panels.renderColorSwatch)
  const strokeSwatch = page.getByTestId('render-stroke-color-swatch')
  const fontSize = page.getByTestId('render-font-size')
  const strokeWidth = page.getByTestId('render-stroke-width')

  const getSwatchBg = (el: Element) => {
    if (!(el instanceof HTMLElement)) return ''
    return el.style.backgroundColor
  }

  const sourceColor = await swatch.evaluate(getSwatchBg)
  const sourceStrokeColor = await strokeSwatch.evaluate(getSwatchBg)
  const sourceFontSize = await fontSize.inputValue()
  const sourceStrokeWidth = await strokeWidth.inputValue()

  await copyBtn.click()

  await page.getByTestId(selectors.panels.textBlockCard(1)).click()
  await expect(scopeIndicator).toHaveText(/Block 2/)

  await pasteBtn.click()

  await expect
    .poll(async () => swatch.evaluate(getSwatchBg), { timeout: 10_000 })
    .toBe(sourceColor)
  await expect
    .poll(async () => strokeSwatch.evaluate(getSwatchBg), { timeout: 5_000 })
    .toBe(sourceStrokeColor)
  await expect(fontSize).toHaveValue(sourceFontSize)
  await expect(strokeWidth).toHaveValue(sourceStrokeWidth)
  await expect(boldToggle).toHaveClass(/bg-primary/)
  await expect(alignRight).toHaveClass(/bg-primary/)
})
