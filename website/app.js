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

  const versionEl = document.querySelector("[data-aos-version]");
  const version = versionEl?.textContent?.trim();
  if (version) {
    const tag = `v${version}`;
    const releaseBase = `https://github.com/azerothl/akasha-os/releases/download/${tag}`;
    const assets = {
      windows: `AgentOS-Preview-${version}-windows-x64.zip`,
      linux: `AgentOS-Preview-${version}-linux-x64.tar.gz`,
      macos: `AgentOS-Preview-${version}-macos-arm64.zip`,
    };
    document.querySelectorAll("[data-dl]").forEach((link) => {
      const platform = link.getAttribute("data-dl");
      const file = assets[platform];
      if (file) {
        link.href = `${releaseBase}/${file}`;
      }
    });
  }
})();
