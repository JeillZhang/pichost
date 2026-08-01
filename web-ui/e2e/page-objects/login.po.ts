import type { Page } from '@playwright/test'

export class LoginPage {
  constructor(private readonly page: Page) {}

  get usernameInput() {
    return this.page.getByLabel(/username/i)
  }

  get passwordInput() {
    return this.page.getByLabel(/password/i)
  }

  get signInButton() {
    return this.page.getByRole('button', { name: /sign in/i })
  }

  get registerLink() {
    return this.page.getByRole('link', { name: /register/i })
  }

  async goto() {
    await this.page.goto('/login')
  }

  async login(username: string, password: string) {
    await this.usernameInput.fill(username)
    await this.passwordInput.fill(password)
    await this.signInButton.click()
  }
}
