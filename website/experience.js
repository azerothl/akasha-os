/**
 * Landing scroll engine — sticky sections, field blend, chrome, preloader.
 * Form borrowed from Abyssal; product truth is Akasha OS Preview.
 */
import Lenis from "./vendor/lenis.mjs";
import {
  PRESETS,
  SECTION_COUNT,
  blendField,
  cssRGB,
  makeBlendedField,
  mountField,
} from "./field.js";

const HOLD = 0.34;
const SECTION_DEPTHS = [0, 200, 800, 1600, 3200, 4800, 6000];

export const view = {
  progress: 0,
  index: 0,
  blend: 0,
  local: 0,
  pointerX: 0,
  pointerY: 0,
  smoothX: 0,
  smoothY: 0,
  velocity: 0,
  ready: false,
  reduced: false,
  intro: 0,
  field: makeBlendedField(),
};

function clamp(v, a, b) {
  return v < a ? a : v > b ? b : v;
}

function smoothstep(edge0, edge1, x) {
  const t = clamp((x - edge0) / (edge1 - edge0), 0, 1);
  return t * t * (3 - 2 * t);
}

function lerp(a, b, t) {
  return a + (b - a) * t;
}

function applySectionScroll(raw) {
  const r = clamp(raw, 0, SECTION_COUNT);
  const idx = clamp(Math.floor(r), 0, SECTION_COUNT - 1);
  const local = clamp(r - idx, 0, 1);
  view.index = idx;
  view.local = local;
  view.blend = smoothstep(HOLD, 1, local);
  view.progress = clamp(r / SECTION_COUNT, 0, 1);
}

function contentVisibility(local) {
  return 1 - smoothstep(0.46, 0.72, local);
}

function currentDepth() {
  const a = SECTION_DEPTHS[view.index] ?? 0;
  const b = SECTION_DEPTHS[Math.min(view.index + 1, SECTION_DEPTHS.length - 1)] ?? a;
  return Math.round(lerp(a, b, view.blend));
}

function revealPanel(el, reduced) {
  const lines = el.querySelectorAll(".line-in");
  const fades = el.querySelectorAll("[data-fade]");
  if (reduced) {
    lines.forEach((n) => {
      n.style.transform = "translateY(0)";
      n.style.opacity = "1";
    });
    fades.forEach((n) => {
      n.style.opacity = "1";
      n.style.transform = "translateY(0)";
    });
    return;
  }
  lines.forEach((n, i) => {
    n.style.transition = "none";
    n.style.transform = "translateY(108%)";
    requestAnimationFrame(() => {
      n.style.transition = `transform 1.05s cubic-bezier(0.16, 1, 0.3, 1) ${i * 0.075}s`;
      n.style.transform = "translateY(0)";
    });
  });
  fades.forEach((n, i) => {
    n.style.transition = "none";
    n.style.opacity = "0";
    n.style.transform = "translateY(12px)";
    requestAnimationFrame(() => {
      n.style.transition = `opacity 0.9s cubic-bezier(0.16, 1, 0.3, 1) ${0.22 + i * 0.08}s, transform 0.9s cubic-bezier(0.16, 1, 0.3, 1) ${0.22 + i * 0.08}s`;
      n.style.opacity = "1";
      n.style.transform = "translateY(0)";
    });
  });
}

function runPreloader() {
  const root = document.querySelector("[data-preloader]");
  const bar = document.querySelector("[data-pre-bar]");
  const num = document.querySelector("[data-pre-depth]");
  const label = document.querySelector("[data-pre-label]");
  if (!root) {
    view.intro = 1;
    view.ready = true;
    document.dispatchEvent(new CustomEvent("akasha:ready"));
    return Promise.resolve();
  }

  if (view.reduced) {
    root.remove();
    view.intro = 1;
    view.ready = true;
    document.dispatchEvent(new CustomEvent("akasha:ready"));
    return Promise.resolve();
  }

  document.documentElement.style.overflow = "hidden";
  const duration = 2000;
  const start = performance.now();

  return new Promise((resolve) => {
    let lastLabel = -1;
    const tick = (now) => {
      const t = clamp((now - start) / duration, 0, 1);
      const eased = t * t * (3 - 2 * t);
      const v = Math.round(eased * 6000);
      if (num) num.textContent = v.toLocaleString("en-GB");
      if (bar) bar.style.transform = `scaleX(${eased})`;
      const idx = Math.min(PRESETS.length - 1, Math.floor(eased * PRESETS.length));
      if (idx !== lastLabel && label) {
        lastLabel = idx;
        label.textContent = PRESETS[idx].label.toUpperCase();
      }
      if (t < 1) {
        requestAnimationFrame(tick);
        return;
      }
      root.style.transition = "clip-path 1.1s cubic-bezier(0.87, 0, 0.13, 1)";
      root.style.clipPath = "inset(0% 0% 100% 0%)";
      const introStart = performance.now();
      const introTick = (tNow) => {
        const it = clamp((tNow - introStart) / 2000, 0, 1);
        view.intro = 1 - Math.pow(1 - it, 2);
        if (it < 1) {
          requestAnimationFrame(introTick);
          return;
        }
        view.intro = 1;
        view.ready = true;
        document.documentElement.style.removeProperty("overflow");
        root.remove();
        document.dispatchEvent(new CustomEvent("akasha:ready"));
        resolve();
      };
      requestAnimationFrame(introTick);
      setTimeout(() => {
        /* ensure resolve if intro path stalls */
      }, 3200);
    };
    requestAnimationFrame(tick);
  });
}

function visibleCloserLines(closer) {
  return Array.from(closer.querySelectorAll(".closer-line")).filter(
    (el) => getComputedStyle(el).display !== "none",
  );
}

function initCloser(panels) {
  const closer = panels[panels.length - 1];
  if (!closer || closer.dataset.layout !== "closer") return null;

  return (raw) => {
    const lines = visibleCloserLines(closer);
    if (!lines.length) return;
    const last = SECTION_COUNT - 1;
    const local = clamp(raw - last, 0, 1);
    lines.forEach((el, i) => {
      const start = i / lines.length;
      const end = (i + 1) / lines.length;
      let op = 0;
      if (local >= start && local < end) {
        const t = (local - start) / (end - start);
        op = t < 0.5 ? t * 2 : 1 - (t - 0.5) * 2;
        op = Math.max(0, Math.min(1, op));
      }
      el.style.opacity = String(op);
      el.style.transform = `translateY(${(1 - op) * 8}px)`;
    });
  };
}

function initScroll() {
  const root = document.documentElement;
  let stage = null;
  let panels = [];
  let railFill = null;
  let depthOut = null;
  let sectionH = 1;
  let stageTop = 0;
  let lastActive = -1;
  let lastDepth = -1;
  let lastAccent = "";
  let closerApply = null;

  const measure = () => {
    stage = document.getElementById("stage");
    panels = Array.from(document.querySelectorAll("[data-panel]"));
    railFill = document.querySelector("[data-rail-fill]");
    depthOut = document.querySelector("[data-depth]");
    if (!stage) return;
    stageTop = stage.getBoundingClientRect().top + window.scrollY;
    sectionH = stage.offsetHeight / SECTION_COUNT;
    closerApply = initCloser(panels);
  };

  const apply = (scrollY) => {
    if (!stage || sectionH <= 0) return;
    const raw = (scrollY - stageTop) / sectionH;
    applySectionScroll(raw);

    const a = PRESETS[view.index];
    const b = PRESETS[Math.min(view.index + 1, PRESETS.length - 1)];
    blendField(view.field, a, b, view.blend);

    if (view.index !== lastActive) {
      lastActive = view.index;
      document.dispatchEvent(new CustomEvent("akasha:section", { detail: lastActive }));
      const panel = panels[lastActive];
      if (panel) revealPanel(panel, view.reduced);
    }

    const accent = cssRGB(view.field.hot);
    if (accent !== lastAccent) {
      lastAccent = accent;
      root.style.setProperty("--accent", accent);
    }

    for (let i = 0; i < panels.length; i++) {
      const local = clamp(raw - i, 0, 1);
      const inPlay = raw >= i - 0.001 && raw < i + 1;
      const vis = inPlay ? contentVisibility(local) : 0;
      const el = panels[i];
      el.style.opacity = vis.toFixed(3);
      el.style.transform = `translate3d(0, ${((1 - vis) * 22).toFixed(2)}px, 0)`;
      el.style.visibility = vis < 0.004 ? "hidden" : "visible";
      el.classList.toggle("is-live", inPlay && vis > 0.05);
      el.style.pointerEvents = inPlay && vis > 0.05 ? "auto" : "none";
    }

    if (railFill) {
      railFill.style.transform = `scaleX(${view.progress.toFixed(4)})`;
    }

    const depth = currentDepth();
    if (depthOut && depth !== lastDepth) {
      lastDepth = depth;
      depthOut.textContent = depth.toLocaleString("en-GB");
    }

    if (closerApply) closerApply(raw);
  };

  measure();
  apply(window.scrollY);

  window.addEventListener(
    "pointermove",
    (e) => {
      view.pointerX = (e.clientX / window.innerWidth) * 2 - 1;
      view.pointerY = -((e.clientY / window.innerHeight) * 2 - 1);
    },
    { passive: true },
  );

  let lenis = null;
  let rafId = 0;

  document.addEventListener("akasha:goto", (e) => {
    const index = clamp(e.detail, 0, SECTION_COUNT - 1);
    const to = stageTop + index * sectionH + sectionH * 0.05;
    if (lenis) lenis.scrollTo(to, { duration: 1.5 });
    else window.scrollTo({ top: to, behavior: "smooth" });
  });

  document.addEventListener("akasha:ready", () => {
    measure();
    apply(window.scrollY);
    document.dispatchEvent(new CustomEvent("akasha:section", { detail: view.index }));
    const panel = panels[view.index];
    if (panel) revealPanel(panel, view.reduced);
  });

  document.querySelectorAll("[data-goto]").forEach((btn) => {
    btn.addEventListener("click", () => {
      const index = Number(btn.getAttribute("data-goto"));
      if (!Number.isNaN(index)) {
        document.dispatchEvent(new CustomEvent("akasha:goto", { detail: index }));
      }
    });
  });

  const navItems = document.querySelectorAll("[data-nav-section]");
  document.addEventListener("akasha:section", (e) => {
    const active = e.detail;
    navItems.forEach((el) => {
      const i = Number(el.getAttribute("data-nav-section"));
      const on = i === active;
      el.setAttribute("data-active", on ? "true" : "false");
      if (on) el.setAttribute("aria-current", "true");
      else el.removeAttribute("aria-current");
    });
  });

  if (view.reduced) {
    window.addEventListener("scroll", () => apply(window.scrollY), { passive: true });
    window.addEventListener("resize", () => {
      measure();
      apply(window.scrollY);
    });
    return;
  }

  lenis = new Lenis({
    duration: 1.15,
    smoothWheel: true,
  });
  lenis.on("scroll", ({ scroll, velocity }) => {
    view.velocity = clamp(Math.abs(velocity) * 0.02, 0, 1);
    apply(scroll);
  });

  const raf = (time) => {
    lenis.raf(time);
    rafId = requestAnimationFrame(raf);
  };
  rafId = requestAnimationFrame(raf);

  window.addEventListener("resize", () => {
    measure();
    apply(lenis.scroll);
  });

  window.addEventListener("beforeunload", () => {
    cancelAnimationFrame(rafId);
    lenis?.destroy();
  });
}

function initCursor() {
  /* Custom cursor stays off: fixed overlays fought hit-testing for chrome
     controls and CTAs in real pointer paths. Native cursor keeps links usable. */
  const dot = document.querySelector("[data-cursor]");
  const ring = document.querySelector("[data-cursor-ring]");
  dot?.remove();
  ring?.remove();
  delete document.body.dataset.cursor;
}

async function main() {
  if (!document.body.classList.contains("experience")) return;
  view.reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  const canvas = document.querySelector("[data-field]");
  if (canvas) {
    setTimeout(() => mountField(canvas, view), 80);
  }

  initCursor();
  initScroll();
  await runPreloader();
}

main();
