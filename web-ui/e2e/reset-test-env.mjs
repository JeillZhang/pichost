/**
 * Resets the E2E test database + Redis, then exits. Called from the Playwright
 * webServer command BEFORE the backend starts, so migrations always run on an
 * empty schema (first-user-is-admin and image counts are deterministic).
 *
 * Playwright starts webServer before globalSetup, so the reset cannot live in
 * global-setup.ts — the backend would already be connected to the old schema.
 *
 * Container detection: docker exec with common compose names first, then local
 * CLI tools (psql / redis-cli). In CI the service containers are already fresh,
 * so failures here are warnings, never fatal.
 */
import { spawnSync } from 'node:child_process'

function tryRun(cmd, args, label) {
  const res = spawnSync(cmd, args, { encoding: 'utf8' })
  if (res.status === 0) return true
  console.warn(
    `[reset] ${label} failed (${cmd}): ${(res.stderr || res.stdout || '').trim().slice(0, 200)}`,
  )
  return false
}

const dbUrl = process.env.PICHOST_DATABASE_URL
const redisUrl = process.env.PICHOST_REDIS_URL

if (!dbUrl || !redisUrl) {
  console.warn('[reset] PICHOST_DATABASE_URL / PICHOST_REDIS_URL not set — skipping reset')
  process.exit(0)
}

const pg = new URL(dbUrl)
const dbName = pg.pathname.slice(1)
const dbUser = pg.username || 'postgres'
const dbPass = pg.password || ''
const redisDb = new URL(redisUrl).pathname.slice(1) || '0'

const pgSql = 'DROP SCHEMA public CASCADE; CREATE SCHEMA public;'
const pgOk =
  tryRun(
    'docker',
    ['exec', '-e', `PGPASSWORD=${dbPass}`, 'pichost-rust-postgres-1', 'psql', '-U', dbUser, '-d', dbName, '-c', pgSql],
    'postgres docker exec',
  ) ||
  tryRun('psql', [dbUrl, '-c', pgSql], 'postgres local psql')

const redisOk =
  tryRun('docker', ['exec', 'pichost-rust-redis-1', 'redis-cli', '-n', redisDb, 'FLUSHDB'], 'redis docker exec') ||
  tryRun('redis-cli', ['-n', redisDb, 'FLUSHDB'], 'redis local cli')

// The admin System Config page reads config.toml from the backend's cwd
// (repo root) — materialize one so GET /admin/config returns real values
// and "Test Connection" can actually connect.
const configToml = `[server]
public_url = "${process.env.PICHOST_SERVER__PUBLIC_URL || 'http://localhost:3000'}"

[database]
url = "${dbUrl}"

[redis]
url = "${redisUrl}"

[auth]
jwt_secret = "${process.env.PICHOST_AUTH__JWT_SECRET || ''}"

[storage]
default_backend = "local"
local_base_path = "${process.env.PICHOST_STORAGE__LOCAL_BASE_PATH || './storage-local'}"
`
try {
  const fs = await import('node:fs')
  fs.writeFileSync(new URL('../../config.toml', import.meta.url), configToml)
  console.log('[reset] wrote config.toml for admin config page')
} catch (e) {
  console.warn('[reset] failed to write config.toml:', e.message)
}

if (pgOk && redisOk) console.log('[reset] test database + redis flushed')
process.exit(0)
