import { type Page } from '@playwright/test'

export class AdminPage {
  constructor(private readonly page: Page) {}

  async goto() {
    await this.page.goto('/admin')
  }

  /** Tab bar buttons: Overview / Users / Invites / System Config. */
  async openTab(label: string) {
    await this.page.getByRole('button', { name: label }).click()
  }

  // ── Overview tab ──
  get statCards() {
    return this.page.locator('div.grid div')
  }

  get totalUsersStat() {
    return this.page.getByText(/total users/i)
  }

  get totalImagesStat() {
    return this.page.getByText(/total images/i)
  }

  // ── Users tab ──
  get userRows() {
    return this.page.locator('table tbody tr')
  }

  /** Row actions are icon-only buttons (Pencil / Trash2). */
  editButtonFor(username: string) {
    return this.page
      .locator('tr', { hasText: username })
      .locator('button:has(.lucide-pencil)')
  }

  deleteButtonFor(username: string) {
    return this.page
      .locator('tr', { hasText: username })
      .locator('button:has(.lucide-trash-2)')
  }

  // ── Invites tab ──
  get createCodeButton() {
    return this.page.getByRole('button', { name: /create code/i })
  }

  get inviteRows() {
    return this.page.locator('table tbody tr')
  }

  // ── System Config tab ──
  get databaseUrlInput() {
    return this.page.getByPlaceholder(/postgres:\/\//i)
  }

  get redisUrlInput() {
    return this.page.getByPlaceholder(/redis:\/\//i)
  }

  get testDatabaseButton() {
    return this.page.getByRole('button', { name: /test connection/i }).first()
  }

  get backupConfigButton() {
    return this.page.getByRole('button', { name: /backup current config/i })
  }
}
