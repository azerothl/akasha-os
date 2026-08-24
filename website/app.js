(function () {
  const storageKey = "aos-lang";
  const supported = ["en", "fr"];

  function currentLang() {
    const fromQuery = new URLSearchParams(window.location.search).get("lang");
    if (supported.includes(fromQuery)) {
      return fromQuery;
    }
    const stored = window.localStorage.getItem(storageKey);
    if (supported.includes(stored)) {
      return stored;
    }
    return "en";
  }

  function applyLang(lang) {
    const next = supported.includes(lang) ? lang : "en";
    document.documentElement.lang = next;
    window.localStorage.setItem(storageKey, next);
    document.querySelectorAll("[data-set-lang]").forEach((button) => {
      button.setAttribute(
        "aria-pressed",
        button.getAttribute("data-set-lang") === next ? "true" : "false",
      );
    });
    applyDownloadLinks(next);
  }

  const VERSION_RE = /^[0-9]+(\.[0-9]+)*$/;
  const RELEASE_REPO = "azerothl/akasha-os";
  const DOWNLOAD_PLATFORMS = ["windows", "linux", "macos"];

  function readProductVersion() {
    const versionEl = document.querySelector("[data-aos-version]");
    const raw = versionEl?.getAttribute("data-aos-version")?.trim();
    if (!raw || !VERSION_RE.test(raw)) {
      return null;
    }
    return raw;
  }

  function releaseAssetUrl(version, fileName) {
    const tag = `v${version}`;
    return new URL(
      fileName,
      `https://github.com/${RELEASE_REPO}/releases/download/${tag}/`,
    ).href;
  }

  function applyDownloadLinks(lang) {
    const version = readProductVersion();
    if (!version) {
      return;
    }
    const assets = {
      windows: `AgentOS-Preview-${version}-windows-x64.zip`,
      linux: `AgentOS-Preview-${version}-linux-x64.tar.gz`,
      macos: `AgentOS-Preview-${version}-macos-arm64.zip`,
    };
    document.querySelectorAll("[data-dl]").forEach((link) => {
      const platform = link.getAttribute("data-dl");
      if (!platform || !DOWNLOAD_PLATFORMS.includes(platform)) {
        return;
      }
      const file = assets[platform];
      if (!file) {
        return;
      }
      link.href = releaseAssetUrl(version, file);
      if (platform === "macos") {
        link.title =
          lang === "fr"
            ? `${file} — non signé ; Gatekeeper avertira`
            : `${file} — unsigned; Gatekeeper will warn`;
      } else {
        link.title = file;
      }
    });
  }

  applyLang(currentLang());

  document.querySelectorAll("[data-set-lang]").forEach((button) => {
    button.addEventListener("click", () => {
      const lang = button.getAttribute("data-set-lang");
      applyLang(lang);
      const url = new URL(window.location.href);
      url.searchParams.set("lang", lang);
      window.history.replaceState({}, "", url);
    });
  });
})();
