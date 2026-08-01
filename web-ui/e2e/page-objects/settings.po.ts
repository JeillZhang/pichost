import { type Page } from '@playwright/test'

type SectionId =
  | 'profile'
  | 'password'
  | 'storage-usage'
  | 'storage-configs'
  | 'watermark'
  | 'preprocessing'
  | 'oauth'

export class SettingsPage {
  constructor(private readonly page: Page) {}

  /** Settings inputs have no id/htmlFor — anchor on the label text. */
  get usernameInput() {
    return this.page.locator('label', { hasText: /^username$/i }).locator('xpath=following-sibling::input')
  }

  get emailInput() {
    return this.page.locator('label', { hasText: /^email$/i }).locator('xpath=following-sibling::input')
  }

  get saveProfileButton() {
    return this.page.getByRole('button', { name: /save profile/i })
  }

  get currentPasswordInput() {
    return this.page.locator('label', { hasText: /current password/i }).locator('xpath=following-sibling::input')
  }

  get newPasswordInput() {
    return this.page.locator('label', { hasText: /new password/i }).locator('xpath=following-sibling::input')
  }

  get changePasswordButton() {
    return this.page.getByRole('button', { name: /change password/i })
  }

  async goto(section: SectionId = 'profile') {
    await this.page.goto(`/settings#settings?section=${section}`)
  }

  /** Section nav buttons are labelled with their title (e.g. "Storage Backends"). */
  async openSectionByTitle(title: string) {
    await this.page.locator('nav button').filter({ hasText: title }).click()
  }

  async saveProfile(username: string, email: string) {
    await this.usernameInput.fill(username)
    await this.emailInput.fill(email)
    await this.saveProfileButton.click()
  }

  async changePassword(current: string, next: string) {
    await this.currentPasswordInput.fill(current)
    await this.newPasswordInput.fill(next)
    await this.changePasswordButton.click()
  }
}
