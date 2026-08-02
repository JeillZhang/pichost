import { type Page } from '@playwright/test'

export class GalleryPage {
  constructor(private readonly page: Page) {}

  get searchInput() {
    return this.page.getByPlaceholder(/search by filename/i)
  }

  get sortDropdown() {
    return this.page.getByRole('combobox', { name: 'Sort images' })
  }

  get emptyState() {
    return this.page.getByText(/no images found/i)
  }

  get selectAllButton() {
    return this.page.getByRole('button', { name: /select all/i })
  }

  get deselectAllButton() {
    return this.page.getByRole('button', { name: /deselect all/i })
  }

  get deleteButton() {
    return this.page.getByRole('button', { name: /^delete$/i })
  }

  get confirmDeleteButton() {
    return this.page.getByRole('button', { name: /^delete$/i }).last()
  }

  /** Image tiles are <button> elements whose inner <img> has alt=original_name. */
  get imageTiles() {
    return this.page.locator('button:has(img[alt])')
  }

  /** Per-tile selection checkbox (always rendered; entry into select mode). */
  selectButtonFor(index = 0) {
    return this.page.locator('button[aria-label^="Select "]').nth(index)
  }

  async goto() {
    await this.page.goto('/gallery')
  }

  async openFirstImage() {
    await this.imageTiles.first().click()
  }

  /** Enters select mode by clicking the tile's selection checkbox. */
  async selectImage(index = 0) {
    await this.selectButtonFor(index).click()
  }
}
