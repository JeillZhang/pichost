#!/usr/bin/env bash
# scripts/tests/release_ci_test.sh — release.yml 双架构矩阵 + deb/rpm 打包断言
set -euo pipefail
# 用法: bash scripts/tests/release_ci_test.sh
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WF="$ROOT/.github/workflows/release.yml"

grep -q 'x86_64-unknown-linux-gnu' "$WF" || { echo "FAIL: amd64 target missing"; exit 1; }
grep -q 'aarch64-unknown-linux-gnu' "$WF" || { echo "FAIL: arm64 target missing"; exit 1; }
grep -q 'zigbuild' "$WF" || { echo "FAIL: zigbuild missing"; exit 1; }
grep -q 'cargo deb' "$WF" || { echo "FAIL: cargo deb missing"; exit 1; }
grep -q 'generate-rpm' "$WF" || { echo "FAIL: generate-rpm missing"; exit 1; }
grep -q 'cargo install cargo-deb' "$WF" || { echo "FAIL: cargo-deb install missing"; exit 1; }
grep -q 'cargo install cargo-generate-rpm' "$WF" \
  || { echo "FAIL: cargo-generate-rpm install missing"; exit 1; }
grep -q 'cargo clippy --workspace -- -D warnings' "$WF" \
  || { echo "FAIL: clippy gate lost"; exit 1; }
grep -q 'cargo test --workspace' "$WF" || { echo "FAIL: test gate lost"; exit 1; }
grep -q 'upload-artifact' "$WF" || { echo "FAIL: artifact upload lost"; exit 1; }

# macos job 与 formula 模板断言
grep -q 'macos-14' "$WF" || { echo "FAIL: macos runner missing"; exit 1; }
grep -q 'aarch64-apple-darwin' "$WF" || { echo "FAIL: arm64-darwin target missing"; exit 1; }
grep -q 'x86_64-apple-darwin' "$WF" || { echo "FAIL: x86_64-darwin target missing"; exit 1; }
grep -q 'lipo -create' "$WF" || { echo "FAIL: lipo missing"; exit 1; }
grep -q 'darwin-universal.tar.gz' "$WF" || { echo "FAIL: universal tarball missing"; exit 1; }
FML="$ROOT/packaging/homebrew/pichost.rb.tpl"
grep -q '__VERSION__' "$FML" || { echo "FAIL: formula version placeholder missing"; exit 1; }
grep -q '__SHA256__' "$FML" || { echo "FAIL: formula sha placeholder missing"; exit 1; }
grep -q 'service do' "$FML" || { echo "FAIL: formula service block missing"; exit 1; }
grep -q 'pichost-start' "$FML" || { echo "FAIL: formula launcher missing"; exit 1; }
grep -q 'SecureRandom' "$FML" || { echo "FAIL: formula jwt gen missing"; exit 1; }
grep -q 'def caveats' "$FML" || { echo "FAIL: formula caveats missing"; exit 1; }
grep -q 'PICHOST_STATIC_DIR' "$FML" || { echo "FAIL: formula env missing"; exit 1; }
grep -q -- '--help' "$FML" || { echo "FAIL: formula test missing"; exit 1; }

# windows job 与 NSIS 断言
grep -q 'windows-latest' "$WF" || { echo "FAIL: windows runner missing"; exit 1; }
grep -q 'x86_64-pc-windows-msvc' "$WF" || { echo "FAIL: windows target missing"; exit 1; }
grep -q 'makensis' "$WF" || { echo "FAIL: makensis missing"; exit 1; }
grep -q -- '-NOCD' "$WF" || { echo "FAIL: makensis -NOCD missing"; exit 1; }
grep -q 'PicHost-setup' "$WF" || { echo "FAIL: installer artifact missing"; exit 1; }
NSI="$ROOT/packaging/windows/installer.nsi"
grep -q 'PROGRAMFILES64' "$NSI" || { echo "FAIL: nsi install dir missing"; exit 1; }
grep -q 'ifndef INSTALLER_VERSION' "$NSI" || { echo "FAIL: nsi version define not guarded"; exit 1; }
grep -q -- '--install-service' "$NSI" || { echo "FAIL: nsi install-service missing"; exit 1; }
grep -q -- '--uninstall-service' "$NSI" || { echo "FAIL: nsi uninstall-service missing"; exit 1; }
grep -q 'ProgramData' "$NSI" || { echo "FAIL: nsi data retention missing"; exit 1; }
grep -q 'RequestExecutionLevel' "$NSI" || { echo "FAIL: nsi admin missing"; exit 1; }

# publish-repo job 与 tap 更新断言
grep -q 'publish-repo' "$WF" || { echo "FAIL: publish job missing"; exit 1; }
grep -q 'APT_GPG_PRIVATE_KEY' "$WF" || { echo "FAIL: gpg secret missing"; exit 1; }
grep -q 'id: import-gpg' "$WF" || { echo "FAIL: gpg import step id missing"; exit 1; }
grep -q 'publish-repo.sh' "$WF" || { echo "FAIL: publish script call missing"; exit 1; }
grep -q 'GPG_PASSPHRASE' "$WF" || { echo "FAIL: gpg passphrase env missing"; exit 1; }
grep -q 'peaceiris/actions-gh-pages' "$WF" || { echo "FAIL: pages action missing"; exit 1; }
grep -q 'jeillzhang/pichost-repo' "$WF" || { echo "FAIL: pages repo missing"; exit 1; }
grep -q 'PICHOST_REPO_PAT' "$WF" || { echo "FAIL: PAT secret missing"; exit 1; }
grep -q 'homebrew-tap' "$WF" || { echo "FAIL: tap update missing"; exit 1; }
grep -q '__SHA256__' "$WF" || { echo "FAIL: formula sha substitution missing"; exit 1; }
grep -q 'testing' "$WF" || { echo "FAIL: prerelease suite routing missing"; exit 1; }

# winget workflow 与 manifest 断言
WINGET="$ROOT/.github/workflows/winget.yml"
grep -q 'release' "$WINGET" || { echo "FAIL: winget trigger missing"; exit 1; }
grep -q 'windows-latest' "$WINGET" || { echo "FAIL: winget runner missing"; exit 1; }
grep -q 'WINGET_CREATE_GITHUB_TOKEN' "$WINGET" || { echo "FAIL: winget token missing"; exit 1; }
grep -q 'wingetcreate.exe update PicHost.PicHost' "$WINGET" \
  || { echo "FAIL: winget update missing"; exit 1; }
grep -q -- '--submit' "$WINGET" || { echo "FAIL: winget submit missing"; exit 1; }
MAN="$ROOT/packaging/winget/manifest.yaml"
grep -q 'PicHost.PicHost' "$MAN" || { echo "FAIL: manifest id missing"; exit 1; }
grep -q 'exe' "$MAN" || { echo "FAIL: manifest type missing"; exit 1; }
grep -q '/S' "$MAN" || { echo "FAIL: manifest silent switch missing"; exit 1; }
echo "release_ci_test.sh PASS"
