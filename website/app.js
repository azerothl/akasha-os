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

  const live = document.querySelector("[data-type]");
  if (!live) {
    return;
  }

  const full = live.getAttribute("data-type") || "";
  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  function finish() {
    live.textContent = full;
  }

  if (reduce || !full) {
    finish();
    return;
  }

  let i = 0;
  live.textContent = "";
  const tick = window.setInterval(() => {
    i += 1;
    live.textContent = full.slice(0, i);
    if (i >= full.length) {
      window.clearInterval(tick);
      finish();
    }
  }, 18);
})();
