# Formule Homebrew — GABARIT, non publié (P8 : seulement sur instruction).
# Tap et dépôt cibles **à confirmer** : `Formula/herdr-core-chat.rb`.
#
# ⚠️ Coexistence (AGENTS §5) : tout diffère d'Herdr Chat (`herdr-chat`) —
# formule, service, binaire, port 8792, données `~/.herdr-core-chat/`.
#
# ⚠️ Les empreintes sont celles de la release GitHub `v<version>` (fichiers
# `.sha256` produits par `xtask release` dans la CI `release-rust`), jamais
# recopiées d'un run à l'autre : le gabarit garde des marqueurs.
class HerdrCoreChat < Formula
  desc "Drive Herdr.app (herdr-core) and its coding agents from your phone"
  homepage "https://github.com/xDelph/herdr-core-chat" # à confirmer
  version "0.2.6"
  license "PolyForm-Noncommercial-1.0.0"

  on_macos do
    on_arm do
      url "https://github.com/xDelph/herdr-core-chat/releases/download/v#{version}/herdr-core-chat-macos-arm64.tar.gz"
      sha256 "A_REMPLACER_PAR_XTASK_RELEASE"
    end
    on_intel do
      url "https://github.com/xDelph/herdr-core-chat/releases/download/v#{version}/herdr-core-chat-macos-x86_64.tar.gz"
      sha256 "A_REMPLACER_PAR_XTASK_RELEASE"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/xDelph/herdr-core-chat/releases/download/v#{version}/herdr-core-chat-linux-aarch64.tar.gz"
      sha256 "A_REMPLACER_PAR_XTASK_RELEASE"
    end
    on_intel do
      url "https://github.com/xDelph/herdr-core-chat/releases/download/v#{version}/herdr-core-chat-linux-x86_64.tar.gz"
      sha256 "A_REMPLACER_PAR_XTASK_RELEASE"
    end
  end

  def install
    bin.install "herdr-core-chat"
    # Le service est un fichier fourni, pas un lancement implicite.
    (share/"herdr-core-chat").install "herdr-core-chat.service" if OS.linux?
  end

  test do
    # Un binaire qui répond à `--version` sans Bun, sans Node, sans base
    # système : c'est la propriété que l'archive doit garantir.
    assert_match "herdr-core-chat", shell_output("#{bin}/herdr-core-chat --version")
  end

  # ⚠️ `brew services` lance le démon sous launchd : PATH minimal, aucun
  # profil de shell lu. Or le démon lance `git`, `tailscale` et les CLI des
  # agents (`pi`, `codex`, `claude` — catalogue des modèles) par leur NOM : sans
  # leurs dossiers dans PATH, le catalogue d'un agent installé hors de Homebrew
  # (`~/.local/bin`, Bun) était introuvable.
  #
  # Les données restent dans le HOME, hors de la Cellar : `~/.herdr-core-chat/`
  # (abonnements push, lectures partagées, journal) — elles survivent aux mises
  # à jour et à `brew reinstall`.
  service do
    run [opt_bin/"herdr-core-chat"]
    keep_alive true
    working_dir Dir.home
    log_path var/"log/herdr-core-chat.log"
    error_log_path var/"log/herdr-core-chat.log"
    environment_variables HERDR_CORE_CHAT_BIND: "127.0.0.1",
                          HERDR_CORE_CHAT_PORT: "8792",
                          PATH: [
                            HOMEBREW_PREFIX/"bin",
                            "#{Dir.home}/.local/bin",
                            "#{Dir.home}/.bun/bin",
                            "/usr/local/bin",
                            "/usr/bin",
                            "/bin",
                            "/usr/sbin",
                            "/sbin",
                          ].join(":")
  end

  def caveats
    <<~EOS
      Start Herdr Core Chat in the background (and at login):
        brew services start herdr-core-chat

      It listens on 127.0.0.1:8792 and talks to herdr-core
      (~/.local/state/herdr-core/default/default.sock). To reach it from your
      phone over Tailscale:
        tailscale serve --bg --https=443 http://127.0.0.1:8792

      It runs alongside Herdr Chat (8791) without sharing anything.
      Data (push subscriptions, read marks, log) lives in ~/.herdr-core-chat/.
      Agent CLIs (pi, codex, claude…) are looked up in Homebrew's bin,
      ~/.local/bin and ~/.bun/bin; set HERDR_CORE_CHAT_* in the service if needed.
    EOS
  end
end
