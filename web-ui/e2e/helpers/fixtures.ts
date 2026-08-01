/**
 * Shared test data constants.
 *
 * NOTE: no shared mutable state here — Playwright runs each spec file in its
 * own worker, so per-file state must live in file-local variables populated
 * by `ensureAuth()` in that file's beforeAll.
 */

export const TEST_ADMIN = {
  username: 'e2e-admin',
  password: 'AdminPass123!',
}

export const TEST_USER = {
  username: 'e2e-user',
  password: 'UserPass123!',
}

/** Fresh username — safe to register even if an earlier run left a row behind. */
export function uniqueUsername(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}`
}

/** Absolute paths to fixture files used in upload tests. */
export const FIXTURE_DIR = new URL('../fixtures/', import.meta.url).pathname
export const FIXTURES = {
  png1x1: `${FIXTURE_DIR}test-1x1.png`,
  png200: `${FIXTURE_DIR}test-200x200.png`,
  invalidTxt: `${FIXTURE_DIR}invalid.txt`,
  /** PNG-named file with text content — passes the client-side accept filter
   *  but fails backend `infer::is_image` validation. */
  fakePng: `${FIXTURE_DIR}fake.png`,
}
