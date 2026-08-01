import { type Page } from '@playwright/test'

export class ImageDetailPage {
  constructor(private readonly page: Page) {}

  /** Inline rename button — the button containing the pencil icon. */
  get nameButton() {
    return this.page.locator('button:has(.lucide-pencil)')
  }

  get renameInput() {
    return this.page.locator('input[type="text"]').first()
  }

  /** Category selector: the select bound to category_id (first select). */
  get categorySelect() {
    return this.page.locator('select').first()
  }

  /** Link format selector: second select (URL/Markdown/HTML/BBCode). */
  get linkFormatSelect() {
    return this.page.locator('select').nth(1)
  }

  /** The <code> element inside the Links LinkCard (main link value). */
  get linkValue() {
    return this.page.locator('code').last()
  }

  get deleteButton() {
    return this.page.getByRole('button', { name: /delete image/i })
  }

  get confirmDeleteButton() {
    return this.page.getByRole('button', { name: /confirm delete/i })
  }

  get backButton() {
    return this.page.getByRole('button', { name: /back/i })
  }

  async goto(id: string) {
    await this.page.goto(`/images/${id}`)
  }

  async rename(newName: string) {
    await this.nameButton.click()
    await this.renameInput.fill(newName)
    await this.renameInput.press('Enter')
  }
}
