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

  function normalizePath(pathname) {
    return pathname.replace(/\\/g, "/");
  }

  function isManualPage() {
    const path = normalizePath(window.location.pathname);
    return /\/docs(\/|$)/.test(path) || /\/install\.html$/.test(path);
  }

  function isInstallPage() {
    return /\/install\.html$/.test(normalizePath(window.location.pathname));
  }

  function manualBase() {
    return isInstallPage() ? "docs/" : "";
  }

  function installHref() {
    return isInstallPage() ? "install.html" : "../install.html";
  }

  function hubHref() {
    return isInstallPage() ? "docs/" : "./";
  }

  function currentManualKey() {
    const path = normalizePath(window.location.pathname);
    if (/\/install\.html$/.test(path)) {
      return "install";
    }
    if (/\/docs\/?$/.test(path) || /\/docs\/index\.html$/.test(path)) {
      return "hub";
    }
    const match = path.match(/\/docs\/([^/]+)\.html$/);
    return match ? match[1] : "";
  }

  function spanLang(en, fr) {
    return `<span data-lang="en">${en}</span><span data-lang="fr">${fr}</span>`;
  }

  function railLink(href, key, en, fr) {
    const current = currentManualKey() === key ? ' aria-current="page"' : "";
    return `<a href="${href}"${current}>${spanLang(en, fr)}</a>`;
  }

  function injectDocsRail() {
    if (!isManualPage()) {
      return;
    }
    const shell = document.querySelector(".shell");
    const main = document.getElementById("content");
    if (!shell || !main || shell.querySelector(".docs-layout")) {
      return;
    }

    document.body.classList.add("docs-manual");

    const base = manualBase();
    const layout = document.createElement("div");
    layout.className = "docs-layout";

    const rail = document.createElement("nav");
    rail.className = "docs-rail";
    rail.setAttribute("aria-label", "Manual");
    rail.innerHTML = `
      <a class="docs-rail-hub" href="${hubHref()}"${currentManualKey() === "hub" ? ' aria-current="page"' : ""}>${spanLang("Manual", "Manuel")}</a>
      <details class="docs-rail-group" open>
        <summary>${spanLang("Start", "Démarrer")}</summary>
        <div class="docs-rail-links">
          ${railLink(installHref(), "install", "Install", "Install")}
          ${railLink(`${base}first-run.html`, "first-run", "First run", "First run")}
        </div>
      </details>
      <details class="docs-rail-group" open>
        <summary>${spanLang("Use", "Utiliser")}</summary>
        <div class="docs-rail-links">
          ${railLink(`${base}use.html`, "use", "Use", "Use")}
          ${railLink(`${base}network.html`, "network", "Network", "Network")}
          ${railLink(`${base}devices.html`, "devices", "Devices", "Périphériques")}
        </div>
      </details>
      <details class="docs-rail-group" open>
        <summary>${spanLang("Extend", "Étendre")}</summary>
        <div class="docs-rail-links">
          ${railLink(`${base}skill.html`, "skill", "Write a skill", "Écrire un skill")}
          ${railLink(`${base}module.html`, "module", "First module", "Premier module")}
          ${railLink(`${base}lan-cluster.html`, "lan-cluster", "LAN cluster", "Cluster LAN")}
        </div>
      </details>
      <details class="docs-rail-group" open>
        <summary>${spanLang("Reference", "Référence")}</summary>
        <div class="docs-rail-links">
          ${railLink(`${base}module-sdk.html`, "module-sdk", "Module SDK", "SDK module")}
          ${railLink(`${base}rich-apps.html`, "rich-apps", "Rich apps", "Apps riches")}
          ${railLink(`${base}build.html`, "build", "Build", "Build")}
        </div>
      </details>
      <details class="docs-rail-group" open>
        <summary>${spanLang("Cohort", "Cohorte")}</summary>
        <div class="docs-rail-links">
          ${railLink(`${base}feedback.html`, "feedback", "Feedback", "Feedback")}
          ${railLink(`${base}limits.html`, "limits", "Limits", "Limits")}
          ${railLink(`${base}whats-new.html`, "whats-new", "What's new", "Nouveauté")}
        </div>
      </details>
    `;

    const active = rail.querySelector("[aria-current='page']");
    if (active) {
      const group = active.closest("details");
      if (group) {
        group.open = true;
      }
    }

    main.parentNode.insertBefore(layout, main);
    layout.appendChild(rail);
    layout.appendChild(main);
  }

  function enhancePageToc() {
    const toc = document.querySelector("nav.page-toc");
    if (!toc) {
      return;
    }
    toc.classList.add("page-toc-ready");
  }

  applyLang(currentLang());
  injectDocsRail();
  enhancePageToc();

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
