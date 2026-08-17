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
})();
