import type { Page } from '@playwright/test'

export class RegisterPage {
  constructor(private readonly page: Page) {}

  get usernameInput() {
    return this.page.getByLabel(/username/i)
  }

  get passwordInput() {
    return this.page.getByLabel(/password/i)
  }

  get inviteCodeInput() {
    return this.page.getByPlaceholder(/invite code/i)
  }

  get registerButton() {
    return this.page.getByRole('button', { name: /register/i })
  }

  async goto() {
    await this.page.goto('/register')
  }

  async register(username: string, password: string, inviteCode?: string) {
    await this.usernameInput.fill(username)
    await this.passwordInput.fill(password)
    if (inviteCode) await this.inviteCodeInput.fill(inviteCode)
    await this.registerButton.click()
  }
}
