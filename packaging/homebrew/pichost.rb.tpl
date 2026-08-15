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
    (libexec/"bin").mkpath
    (libexec/"bin/pichost-start").write <<~EOS
      #!/bin/bash
      # PicHost service launcher:source #{var}/pichost/.env(若存在),然后 exec 真实二进制。
      # 源文件中的 PICHOST_* 变量优先于 service 块的静态 environment_variables。
      set -a
      if [ -f "#{var}/pichost/.env" ]; then
        . "#{var}/pichost/.env"
      fi
      set +a
      exec "#{opt_bin}/pichost-api" "$@"
    EOS
    (libexec/"bin/pichost-start").chmod 0o755
  end

  def post_install
    (var/"pichost").mkpath
    env_file = var/"pichost/.env"
    content = env_file.exist? ? File.read(env_file) : ""
    secret = content[/^PICHOST_AUTH__JWT_SECRET=(.*)$/, 1].to_s.strip
    unless secret.length >= 32
      require "securerandom"
      content += "\n" unless content.empty? || content.end_with?("\n")
      content += "PICHOST_AUTH__JWT_SECRET=#{SecureRandom.hex(32)}\n"
      File.write(env_file, content)
    end
    File.chmod(0o600, env_file)
  end

  service do
    run [libexec/"bin/pichost-start"]
    environment_variables(
      "PICHOST_DATABASE_MODE" => "sqlite",
      "PICHOST_DATABASE_URL" => "sqlite://#{var}/pichost/pichost.db",
      "PICHOST_STORAGE__LOCAL_BASE_PATH" => "#{var}/pichost/storage-local",
      "PICHOST_STATIC_DIR" => "#{share}/pichost/web-ui",
      "PICHOST_SERVER_PUBLIC_URL" => "http://localhost:3000",
    )
    keep_alive true
  end

  def caveats
    <<~EOS
      The pichost service reads its configuration from #{var}/pichost/.env
      (generated at install time with a random PICHOST_AUTH__JWT_SECRET).

      To rotate the JWT secret: edit #{var}/pichost/.env, replace
      PICHOST_AUTH__JWT_SECRET with a new value of at least 32 characters
      (e.g. `openssl rand -hex 32`), then run:
        brew services restart pichost
    EOS
  end

  test do
    system "#{bin}/pichost-api", "--help"
  end
end
