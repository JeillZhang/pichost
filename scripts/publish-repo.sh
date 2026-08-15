#!/bin/bash
# 用法: publish-repo.sh <repo_root> <deb_dir> <rpm_dir> [suite]
#   suite: stable(默认)/testing;GPG_SIGN=1 时签名(CI 已导入私钥),=0 跳过(本地测试)
set -euo pipefail
REPO_ROOT="${1:?repo_root}"; DEB_DIR="${2:?deb_dir}"; RPM_DIR="${3:?rpm_dir}"
SUITE="${4:-stable}"
GPG_SIGN="${GPG_SIGN:-1}"
PASS_ARGS=()
[ -n "${GPG_PASSPHRASE:-}" ] && PASS_ARGS=(--pinentry-mode loopback --passphrase "$GPG_PASSPHRASE")

# ---- apt 仓库 ----
APT="$REPO_ROOT/apt"
mkdir -p "$APT/pool/main/p/pichost" \
    "$APT/dists/$SUITE/main/binary-amd64" "$APT/dists/$SUITE/main/binary-arm64"
cp "$DEB_DIR"/*.deb "$APT/pool/main/p/pichost/"
cd "$APT"
for A in amd64 arm64; do
    apt-ftparchive packages "pool/main" \
        > "dists/$SUITE/main/binary-$A/Packages" 2>/dev/null || dpkg-scanpackages "pool/main" \
        > "dists/$SUITE/main/binary-$A/Packages"
    gzip -9c "dists/$SUITE/main/binary-$A/Packages" \
        > "dists/$SUITE/main/binary-$A/Packages.gz"
done
apt-ftparchive -o "APT::FTPArchive::Release::Origin=PicHost" \
    -o "APT::FTPArchive::Release::Label=PicHost" \
    -o "APT::FTPArchive::Release::Suite=$SUITE" \
    -o "APT::FTPArchive::Release::Codename=$SUITE" \
    -o "APT::FTPArchive::Release::Components=main" \
    -o "APT::FTPArchive::Release::Architectures=amd64 arm64" \
    release "dists/$SUITE" > "dists/$SUITE/Release"
if [ "$GPG_SIGN" = "1" ]; then
    gpg "${PASS_ARGS[@]}" --batch --yes -abs -o "dists/$SUITE/Release.gpg" "dists/$SUITE/Release"
    gpg "${PASS_ARGS[@]}" --batch --yes --clearsign -o "dists/$SUITE/InRelease" "dists/$SUITE/Release"
fi
cd - >/dev/null

# ---- rpm 仓库 ----
RPM="$REPO_ROOT/rpm"
for ARCH in x86_64 aarch64; do
    [ -d "$RPM_DIR/$ARCH" ] || continue
    mkdir -p "$RPM/$ARCH"
    cp "$RPM_DIR/$ARCH"/*.rpm "$RPM/$ARCH/" 2>/dev/null || true
    if command -v createrepo_c >/dev/null 2>&1; then
        createrepo_c --update -q "$RPM/$ARCH"
        if [ "$GPG_SIGN" = "1" ] && [ -f "$RPM/$ARCH/repodata/repomd.xml" ]; then
            gpg "${PASS_ARGS[@]}" --batch --yes -a -o "$RPM/$ARCH/repodata/repomd.xml.asc" \
                --detach-sign "$RPM/$ARCH/repodata/repomd.xml"
        fi
    fi
done

# ---- 公钥(可选,CI 已导出时) ----
if [ "$GPG_SIGN" = "1" ] && [ -n "${GPG_FINGERPRINT:-}" ]; then
    gpg --batch --yes --armor --export "$GPG_FINGERPRINT" > "$REPO_ROOT/public.key"
fi
echo "repo published: $REPO_ROOT (suite=$SUITE, gpg_sign=$GPG_SIGN)"
