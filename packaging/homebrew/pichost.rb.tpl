# frozen_string_literal: true
#
# PicHost Homebrew formula(由 release.yml 的 publish job 渲染后推送到 JeillZhang/homebrew-tap)
class Pichost < Formula
  desc "Self-hosted image hosting server (SQLite-first, zero external deps)"
  homepage "https://github.com/JeillZhang/pichost"
  url "https://github.com/JeillZhang/pichost/releases/download/__TAG__/pichost-__TAG__-darwin-universal.tar.gz"
  sha256 "__SHA256__"
  version "__VERSION__"
  license "MIT"

  depends_on :macos

  def install
    bin.install "pichost-api-universal" => "pichost-api"
    bin.install "pichost-worker-universal" => "pichost-worker"
    (share/"pichost").install Dir["web-ui"]
    (share/"pichost").install Dir["migrations"]
    (share/"pichost").install Dir["migrations-sqlite"]
  end

  def post_install
    (var/"pichost").mkpath
  end

  service do
    run [opt_bin/"pichost-api"]
    environment_variables(
      "PICHOST_DATABASE_MODE" => "sqlite",
      "PICHOST_DATABASE_URL" => "sqlite://#{var}/pichost/pichost.db",
      "PICHOST_STORAGE__LOCAL_BASE_PATH" => "#{var}/pichost/storage-local",
      "PICHOST_STATIC_DIR" => "#{share}/pichost/web-ui",
      "PICHOST_SERVER_PUBLIC_URL" => "http://localhost:3000",
    )
    keep_alive true
  end

  test do
    system "#{bin}/pichost-api", "--help"
  end
end
