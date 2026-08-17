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

  const bodies = [
    { id: "MEMORY", orbit: 0.28, size: 0.026, speed: 0.62, phase: 0.4, kind: "memory" },
    { id: "CAPS", orbit: 0.42, size: 0.032, speed: 0.41, phase: 1.7, kind: "caps" },
    { id: "GPU", orbit: 0.56, size: 0.038, speed: 0.27, phase: 3.3, kind: "gpu" },
    { id: "AGENTS", orbit: 0.72, size: 0.046, speed: 0.16, phase: 5.1, kind: "agents" },
  ];

  function paintOrrery(canvas) {
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const ctx = canvas.getContext("2d", { alpha: false });
    if (!ctx) {
      return;
    }

    const wind = { value: 1, target: 1 };
    const key = document.querySelector(".key");
    if (key && !reduce) {
      const setWind = (target) => {
        wind.target = target;
      };
      key.addEventListener("pointerenter", () => setWind(3.4));
      key.addEventListener("pointerleave", () => setWind(1));
      key.addEventListener("focus", () => setWind(3.4));
      key.addEventListener("blur", () => setWind(1));
    }

    let w = 0;
    let h = 0;
    let t = 1.15;
    let frame = 0;
    let last = 0;
    const plate = document.body.classList.contains("plate");

    function layout() {
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      w = Math.max(1, Math.floor(canvas.clientWidth * dpr));
      h = Math.max(1, Math.floor(canvas.clientHeight * dpr));
      canvas.width = w;
      canvas.height = h;
    }

    function ellipse(cx, cy, rx, ry) {
      ctx.beginPath();
      ctx.ellipse(cx, cy, rx, ry, 0, 0, Math.PI * 2);
      ctx.stroke();
    }

    function draw(now) {
      const dt = last ? Math.min(0.05, (now - last) / 1000) : 0.016;
      last = now;
      if (!reduce) {
        t += dt * wind.value;
        wind.value += (wind.target - wind.value) * 0.08;
      }

      ctx.fillStyle = "#070b14";
      ctx.fillRect(0, 0, w, h);

      const cx = plate ? w * 0.78 : w * 0.34;
      const cy = plate ? h * 0.22 : h * 0.5;
      const span = Math.min(w, h) * (plate ? 0.42 : 0.86);
      const rx = span * 0.5;
      const ry = rx * 0.32;
      const lw = Math.max(1, Math.round(w / 1100));
      const label = canvas.clientWidth >= 640;

      ctx.save();
      ctx.strokeStyle = "rgba(62, 224, 196, 0.28)";
      ctx.lineWidth = lw;
      ctx.beginPath();
      ctx.moveTo(cx, cy - ry * 2.4);
      ctx.lineTo(cx, cy + ry * 2.55);
      ctx.stroke();
      ctx.beginPath();
      ctx.moveTo(cx - span * 0.06, cy + ry * 2.55);
      ctx.lineTo(cx + span * 0.06, cy + ry * 2.55);
      ctx.stroke();
      ctx.beginPath();
      ctx.moveTo(cx - span * 0.018, cy + ry * 2.55);
      ctx.lineTo(cx, cy + ry * 2.85);
      ctx.lineTo(cx + span * 0.018, cy + ry * 2.55);
      ctx.stroke();

      ctx.strokeStyle = "rgba(62, 224, 196, 0.55)";
      bodies.forEach((body) => {
        ellipse(cx, cy, rx * body.orbit, ry * body.orbit);
      });

      ctx.fillStyle = "#3ee0c4";
      ctx.beginPath();
      ctx.arc(cx, cy, Math.max(3, span * 0.014), 0, Math.PI * 2);
      ctx.fill();

      ctx.font = `600 ${Math.max(12, Math.round(span * 0.03))}px "Archivo Narrow", sans-serif`;
      ctx.textAlign = "center";
      ctx.textBaseline = "top";

      function globe(kind, x, y, rad) {
        ctx.save();
        ctx.translate(x, y);
        ctx.fillStyle = "rgba(7, 11, 20, 0.55)";
        ctx.beginPath();
        ctx.ellipse(0, rad * 0.55, rad * 0.85, rad * 0.28, 0, 0, Math.PI * 2);
        ctx.fill();
        if (kind === "memory") {
          ctx.fillStyle = "#dce6f0";
          ctx.beginPath();
          ctx.arc(0, 0, rad, 0, Math.PI * 2);
          ctx.fill();
          ctx.fillStyle = "#070b14";
          ctx.beginPath();
          ctx.arc(-rad * 0.2, -rad * 0.2, rad * 0.28, 0, Math.PI * 2);
          ctx.fill();
        } else if (kind === "caps") {
          ctx.fillStyle = "#3ee0c4";
          ctx.beginPath();
          ctx.arc(0, 0, rad, 0, Math.PI * 2);
          ctx.fill();
          ctx.strokeStyle = "#070b14";
          ctx.lineWidth = Math.max(1.5, rad * 0.18);
          ctx.beginPath();
          ctx.arc(0, 0, rad * 0.42, 0, Math.PI * 2);
          ctx.stroke();
        } else if (kind === "gpu") {
          ctx.fillStyle = "#070b14";
          ctx.beginPath();
          ctx.arc(0, 0, rad, 0, Math.PI * 2);
          ctx.fill();
          ctx.strokeStyle = "#3ee0c4";
          ctx.lineWidth = Math.max(1.5, rad * 0.16);
          ctx.beginPath();
          ctx.arc(0, 0, rad * 0.72, 0, Math.PI * 2);
          ctx.stroke();
          ctx.beginPath();
          ctx.arc(0, 0, rad * 0.38, 0, Math.PI * 2);
          ctx.stroke();
        } else {
          ctx.fillStyle = "#dce6f0";
          ctx.beginPath();
          ctx.arc(0, 0, rad, 0, Math.PI * 2);
          ctx.fill();
          ctx.fillStyle = "#3ee0c4";
          ctx.beginPath();
          ctx.arc(rad * 0.18, -rad * 0.12, rad * 0.72, Math.PI * 0.55, Math.PI * 1.65);
          ctx.fill();
          ctx.strokeStyle = "#070b14";
          ctx.lineWidth = Math.max(1, rad * 0.08);
          ctx.beginPath();
          ctx.arc(0, 0, rad * 0.92, 0, Math.PI * 2);
          ctx.stroke();
        }
        ctx.restore();
      }

      bodies.forEach((body) => {
        const a = t * body.speed + body.phase;
        const x = cx + Math.cos(a) * rx * body.orbit;
        const y = cy + Math.sin(a) * ry * body.orbit;
        const rad = span * body.size;

        ctx.strokeStyle = "rgba(62, 224, 196, 0.5)";
        ctx.lineWidth = lw;
        ctx.beginPath();
        ctx.moveTo(cx, cy);
        ctx.lineTo(x, y);
        ctx.stroke();

        globe(body.kind, x, y, rad);

        if (label) {
          ctx.fillStyle = "#3ee0c4";
          ctx.fillText(body.id, cx, cy + ry * body.orbit + rad * 0.2 + 10);
        }
      });

      ctx.restore();
    }

    function tick(now) {
      draw(now);
      if (!reduce) {
        frame = window.requestAnimationFrame(tick);
      }
    }

    layout();
    draw(0);
    if (!reduce) {
      frame = window.requestAnimationFrame(tick);
    }

    window.addEventListener("resize", () => {
      layout();
      if (reduce) {
        draw(0);
      }
    });

    return function stop() {
      window.cancelAnimationFrame(frame);
    };
  }

  const ether = document.querySelector("[data-ether]");
  if (ether) {
    paintOrrery(ether);
  }
})();
