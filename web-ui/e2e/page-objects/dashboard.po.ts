import { expect, type Page } from '@playwright/test'

export class DashboardPage {
  constructor(private readonly page: Page) {}

  get urlInput() {
    return this.page.getByPlaceholder(/image url/i)
  }

  get uploadButton() {
    return this.page.getByRole('button', { name: /upload/i })
  }

  get uploadsHeading() {
    return this.page.getByRole('heading', { name: /uploads/i })
  }

  get clearDoneButton() {
    return this.page.getByRole('button', { name: /clear done/i })
  }

  get recentImages() {
    return this.page.getByText(/recent images/i)
  }

  get emptyState() {
    return this.page.getByText(/no images yet/i)
  }

  async goto() {
    await this.page.goto('/dashboard')
  }

  /** Uploads a file by setting it directly on the hidden file input. */
  async uploadFile(filePath: string) {
    await this.page.setInputFiles('input[type="file"]', filePath)
  }

  async uploadUrl(url: string) {
    await this.urlInput.fill(url)
    await this.uploadButton.click()
  }

  /** Waits until no upload card is in a non-terminal state. Status labels are
   *  "Uploading…" / "Processing…" / "Pending" (UploadCard STATUS_LABELS); a
   *  broad /processing/i regex would also match the "Preprocessing: Off" tag. */
  async waitForUploadsDone(timeout = 30_000) {
    await expect
      .poll(
        async () =>
          this.page
            .locator('text=/Uploading…|Processing…|^Pending$/')
            .count(),
        { timeout },
      )
      .toBe(0)
  }
}
