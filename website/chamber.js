/**
 * Cloud chamber: ākāśa as the medium. Agent actions ionize tracks.
 * The mark and the tracks draw in as particles; the hydrogen spark sits
 * at the centre of the A. Fixed geometries; motion is particles only.
 * Seed 03349c03.
 */
const VOID = "#070b14";
const ICE = "#5ee7ff";
const SIGNAL = "#2ef0c8";
const HYDROGEN = "#ff5a48";
const PAPER = "#e8eef6";

function mulberry(seed) {
  let t = seed + 0x6d2b79f5;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}

function paintChamber(canvas) {
  const ctx = canvas.getContext("2d", { alpha: false });
  if (!ctx) {
    return;
  }
  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const plate = document.body.classList.contains("plate");
  const lost =
    document.body.classList.contains("sky") && Boolean(document.querySelector("main.lost"));
  const ignite = { value: 1, target: 1 };
  const key = document.querySelector(".key");
  if (key && !reduce) {
    const on = () => {
      ignite.target = 2.4;
    };
    const off = () => {
      ignite.target = 1;
    };
    key.addEventListener("pointerenter", on);
    key.addEventListener("pointerleave", off);
    key.addEventListener("focus", on);
    key.addEventListener("blur", off);
  }

  const fog = Array.from({ length: 110 }, (_, i) => ({
    x: mulberry(i * 17),
    y: mulberry(i * 29 + 3),
    r: 0.3 + mulberry(i * 11) * 1.7,
    a: 0.04 + mulberry(i * 13) * 0.08,
    drift: 0.2 + mulberry(i * 7) * 0.8,
  }));

  function layoutBox() {
    const w = Math.max(1, canvas.clientWidth);
    const h = Math.max(1, canvas.clientHeight);
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    canvas.width = Math.floor(w * dpr);
    canvas.height = Math.floor(h * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    return { w, h };
  }

  function chamberOrigin(w, h) {
    if (plate) {
      return { x: w * 0.72, y: h * 0.28, scale: Math.min(w, h) * 0.24, maxX: w * 0.96 };
    }
    if (lost) {
      return { x: w * 0.42, y: h * 0.58, scale: Math.min(w, h) * 0.44, maxX: w * 0.82 };
    }
    return { x: w * 0.12, y: h * 0.72, scale: Math.min(w, h) * 0.52, maxX: w * 0.52 };
  }

  function sampleCubic(p0, p1, p2, p3, u) {
    const v = 1 - u;
    return {
      x: v * v * v * p0.x + 3 * v * v * u * p1.x + 3 * v * u * u * p2.x + u * u * u * p3.x,
      y: v * v * v * p0.y + 3 * v * v * u * p1.y + 3 * v * u * u * p2.y + u * u * u * p3.y,
    };
  }

  function aPaths(ox, oy, s) {
    const leg = [
      { x: ox, y: oy },
      { x: ox + s * 0.18, y: oy - s * 0.42 },
      { x: ox + s * 0.28, y: oy - s * 0.82 },
      { x: ox + s * 0.38, y: oy - s * 1.12 },
    ];
    const leg2 = [
      { x: ox + s * 0.38, y: oy - s * 1.12 },
      { x: ox + s * 0.48, y: oy - s * 0.82 },
      { x: ox + s * 0.58, y: oy - s * 0.42 },
      { x: ox + s * 0.76, y: oy },
    ];
    const bar1 = [
      { x: ox + s * 0.16, y: oy - s * 0.42 },
      { x: ox + s * 0.34, y: oy - s * 0.54 },
      { x: ox + s * 0.5, y: oy - s * 0.52 },
      { x: ox + s * 0.6, y: oy - s * 0.42 },
    ];
    const bar2 = [
      { x: ox + s * 0.6, y: oy - s * 0.42 },
      { x: ox + s * 0.74, y: oy - s * 0.56 },
      { x: ox + s * 0.82, y: oy - s * 0.32 },
      { x: ox + s * 0.68, y: oy - s * 0.24 },
    ];
    const bar3 = [
      { x: ox + s * 0.68, y: oy - s * 0.24 },
      { x: ox + s * 0.56, y: oy - s * 0.18 },
      { x: ox + s * 0.5, y: oy - s * 0.4 },
      { x: ox + s * 0.62, y: oy - s * 0.42 },
    ];
    return { leg, leg2, bar1, bar2, bar3 };
  }

  function drawParticlesAlong(path, s, color, glow, progress, seedBase) {
    const steps = 26;
    const maxU = Math.min(1, progress);
    ctx.save();
    ctx.lineCap = "round";
    for (let i = 0; i <= steps; i += 1) {
      const u = (i / steps) * maxU;
      const p = sampleCubic(path[0], path[1], path[2], path[3], u);
      const n = mulberry(seedBase + i * 31);
      const r = Math.max(1.1, s * 0.008) * (0.6 + n * 1.1);
      ctx.fillStyle = color;
      ctx.shadowColor = color;
      ctx.shadowBlur = glow * (0.6 + n * 0.8);
      ctx.globalAlpha = 0.35 + n * 0.6;
      ctx.beginPath();
      ctx.arc(p.x + (n - 0.5) * s * 0.012, p.y + (mulberry(seedBase + i * 17) - 0.5) * s * 0.012, r, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.restore();
  }

  function drawA(ox, oy, s, glow, t, reveal) {
    const paths = aPaths(ox, oy, s);
    const wLeg = Math.max(2.4, s * 0.05);
    const wBar = Math.max(1.6, s * 0.03);
    const legReveal = Math.min(1, reveal * 1.6);
    const barReveal = Math.max(0, reveal * 1.6 - 0.6);
    drawParticlesAlong(paths.leg, s, ICE, glow, legReveal, 11);
    drawParticlesAlong(paths.leg2, s, ICE, glow, legReveal, 23);
    if (barReveal > 0) {
      drawParticlesAlong(paths.bar1, s, ICE, glow * 0.8, barReveal, 37);
      drawParticlesAlong(paths.bar2, s, ICE, glow * 0.8, barReveal, 41);
      drawParticlesAlong(paths.bar3, s, ICE, glow * 0.8, barReveal, 53);
    }
    if (legReveal > 0.4) {
      ctx.save();
      ctx.strokeStyle = ICE;
      ctx.shadowColor = ICE;
      ctx.shadowBlur = glow * 0.5;
      ctx.lineWidth = wLeg * 0.4;
      ctx.globalAlpha = 0.5 * legReveal;
      ctx.beginPath();
      ctx.moveTo(ox, oy);
      ctx.bezierCurveTo(ox + s * 0.18, oy - s * 0.42, ox + s * 0.28, oy - s * 0.82, ox + s * 0.38, oy - s * 1.12);
      ctx.bezierCurveTo(ox + s * 0.48, oy - s * 0.82, ox + s * 0.58, oy - s * 0.42, ox + s * 0.76, oy);
      ctx.stroke();
      ctx.restore();
    }
    const spark = sampleCubic(paths.bar1[0], paths.bar1[1], paths.bar1[2], paths.bar1[3], 0.5);
    const pulse = 1 + Math.sin(t * 2.4) * 0.18;
    ctx.beginPath();
    ctx.fillStyle = HYDROGEN;
    ctx.shadowColor = HYDROGEN;
    ctx.shadowBlur = glow * 0.6;
    ctx.arc(spark.x, spark.y, Math.max(3.4, s * 0.034) * pulse * Math.min(1, reveal * 2), 0, Math.PI * 2);
    ctx.fill();
    ctx.shadowBlur = 0;
    ctx.beginPath();
    ctx.fillStyle = ICE;
    ctx.shadowColor = ICE;
    ctx.shadowBlur = glow * 0.3;
    ctx.globalAlpha = 0.5;
    ctx.arc(spark.x, spark.y, Math.max(1.4, s * 0.012) * Math.min(1, reveal * 2), 0, Math.PI * 2);
    ctx.fill();
    ctx.globalAlpha = 1;
    ctx.shadowBlur = 0;
  }

  function label(text, x, y, alpha) {
    ctx.font = "italic 13px Georgia, Palatino Linotype, Palatino, Times New Roman, serif";
    ctx.fillStyle = PAPER;
    ctx.textBaseline = "middle";
    ctx.shadowBlur = 0;
    ctx.globalAlpha = Math.max(0, Math.min(1, alpha));
    ctx.fillText(text, x, y);
    ctx.globalAlpha = 1;
  }

  function drawMemory(ox, oy, s, glow, maxX, reveal) {
    const y = oy - s * 0.62;
    const x1 = Math.min(ox + s * 0.92, maxX - 88);
    const path = [
      { x: ox, y },
      { x: ox + (x1 - ox) * 0.45, y: y - s * 0.06 },
      { x: ox + (x1 - ox) * 0.75, y: y - s * 0.02 },
      { x: x1, y: y + s * 0.01 },
    ];
    drawParticlesAlong(path, s, ICE, glow, reveal, 61);
    if (reveal > 0.9) {
      ctx.save();
      ctx.strokeStyle = ICE;
      ctx.shadowColor = ICE;
      ctx.shadowBlur = glow * 0.4;
      ctx.lineWidth = Math.max(1.1, s * 0.01);
      ctx.lineCap = "round";
      ctx.beginPath();
      ctx.moveTo(x1, y + s * 0.01);
      ctx.quadraticCurveTo(x1 + s * 0.08, y - s * 0.04, x1 + s * 0.16, y - s * 0.11);
      ctx.moveTo(x1, y + s * 0.01);
      ctx.lineTo(x1 + s * 0.2, y + s * 0.01);
      ctx.moveTo(x1, y + s * 0.01);
      ctx.quadraticCurveTo(x1 + s * 0.08, y + s * 0.06, x1 + s * 0.16, y + s * 0.12);
      ctx.stroke();
      ctx.restore();
    }
    label("MEMORY", x1 + s * 0.22, y, reveal - 0.4);
  }

  function drawCaps(ox, oy, s, glow, maxX, reveal) {
    const y = oy - s * 0.28;
    const x1 = Math.min(ox + s * 0.98, maxX - 72);
    const path = [
      { x: ox, y },
      { x: ox + (x1 - ox) * 0.5, y: y + s * 0.05 },
      { x: ox + (x1 - ox) * 0.78, y: y + s * 0.02 },
      { x: x1, y },
    ];
    drawParticlesAlong(path, s, SIGNAL, glow, reveal, 71);
    if (reveal > 0.85) {
      ctx.save();
      ctx.strokeStyle = SIGNAL;
      ctx.shadowColor = SIGNAL;
      ctx.shadowBlur = glow * 0.35;
      ctx.lineWidth = Math.max(1, s * 0.009);
      ctx.lineCap = "round";
      [0.28, 0.52, 0.76].forEach((u, i) => {
        const x = ox + (x1 - ox) * u;
        const dir = i % 2 === 0 ? -1 : 1;
        ctx.beginPath();
        ctx.moveTo(x, y);
        ctx.quadraticCurveTo(x + s * 0.01, y + dir * s * 0.04, x + s * 0.03, y + dir * s * 0.09);
        ctx.stroke();
      });
      ctx.restore();
    }
    label("CAPS", x1 + 12, y, reveal - 0.4);
  }

  function drawGpu(ox, oy, s, glow, maxX, reveal) {
    const y = oy + s * 0.08;
    const x1 = Math.min(ox + s * 0.72, maxX - 100);
    const path = [
      { x: ox, y },
      { x: ox + (x1 - ox) * 0.55, y: y - s * 0.04 },
      { x: ox + (x1 - ox) * 0.82, y: y - s * 0.02 },
      { x: x1, y },
    ];
    drawParticlesAlong(path, s, ICE, glow, reveal, 83);
    if (reveal > 0.9) {
      ctx.save();
      ctx.strokeStyle = ICE;
      ctx.shadowColor = ICE;
      ctx.shadowBlur = glow * 0.4;
      ctx.lineWidth = Math.max(1.1, s * 0.01);
      ctx.lineCap = "round";
      const cx = x1 + s * 0.07;
      const cy = y;
      ctx.beginPath();
      for (let a = 0; a <= Math.PI * 2.35; a += 0.16) {
        const r = s * 0.016 + a * s * 0.026;
        const x = cx + Math.cos(a) * r;
        const yy = cy + Math.sin(a) * r;
        if (a === 0) {
          ctx.moveTo(x, yy);
        } else {
          ctx.lineTo(x, yy);
        }
      }
      ctx.stroke();
      ctx.restore();
    }
    label("GPU", x1 + s * 0.26, y, reveal - 0.4);
  }

  function drawAgents(ox, oy, s, glow, maxX, reveal) {
    const y = oy + s * 0.42;
    const xMid = ox + Math.min(s * 0.42, (maxX - ox) * 0.4);
    const x1 = Math.min(ox + s * 0.95, maxX - 80);
    const path = [
      { x: ox, y },
      { x: (ox + xMid) / 2, y: y - s * 0.03 },
      { x: xMid, y },
      { x: x1, y },
    ];
    drawParticlesAlong(path, s, SIGNAL, glow, reveal, 97);
    if (reveal > 0.85) {
      ctx.save();
      ctx.strokeStyle = SIGNAL;
      ctx.shadowColor = SIGNAL;
      ctx.shadowBlur = glow * 0.35;
      ctx.lineWidth = Math.max(1, s * 0.009);
      ctx.lineCap = "round";
      ctx.beginPath();
      ctx.moveTo(xMid - s * 0.08, y - s * 0.1);
      ctx.quadraticCurveTo(xMid, y + s * 0.14, xMid + s * 0.12, y - s * 0.08);
      ctx.moveTo(xMid - s * 0.04, y + s * 0.1);
      ctx.quadraticCurveTo(xMid + s * 0.06, y - s * 0.12, xMid + s * 0.14, y + s * 0.06);
      ctx.stroke();
      ctx.restore();
    }
    label("AGENTS", x1 + 12, y, reveal - 0.4);
  }

  function travelers(ox, oy, s, t, count, maxX) {
    ctx.save();
    const span = Math.min(s * 0.92, maxX - ox - 40);
    ctx.lineCap = "round";
    for (let i = 0; i < count; i += 1) {
      const u = (t * 0.09 + i / count) % 1;
      const y = oy - s * 0.62 + (i % 4) * s * 0.34;
      const x = ox + u * span;
      const color = i % 2 === 0 ? ICE : SIGNAL;
      ctx.fillStyle = color;
      ctx.shadowColor = color;
      ctx.shadowBlur = 14;
      ctx.beginPath();
      ctx.arc(x, y, 2.2, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.restore();
  }

  function drawFog(w, h, t) {
    ctx.fillStyle = ICE;
    fog.forEach((dot) => {
      ctx.globalAlpha = dot.a * 0.6;
      ctx.beginPath();
      ctx.arc(
        (dot.x + t * 0.004 * dot.drift) % 1 * w,
        dot.y * h + Math.sin(t * 0.4 * dot.drift + dot.x * 9) * 3,
        dot.r,
        0,
        Math.PI * 2,
      );
      ctx.fill();
    });
    ctx.globalAlpha = 1;
  }

  let last = 0;
  let t = 0;
  let box = layoutBox();

  function frame(now) {
    const dt = last ? Math.min(0.05, (now - last) / 1000) : 0.016;
    last = now;
    if (!reduce) {
      t += dt * ignite.value;
      ignite.value += (ignite.target - ignite.value) * 0.08;
    }
    const { w, h } = box;
    ctx.fillStyle = VOID;
    ctx.fillRect(0, 0, w, h);
    drawFog(w, h, t);
    const o = chamberOrigin(w, h);
    const glow = (14 + ignite.value * 12) * (reduce ? 0.35 : 1);
    const reveal = reduce ? 1 : Math.min(1, 0.12 + t * 0.42);
    drawMemory(o.x, o.y, o.scale, glow, o.maxX, reveal);
    drawCaps(o.x, o.y, o.scale, glow, o.maxX, reveal);
    drawGpu(o.x, o.y, o.scale, glow, o.maxX, reveal);
    drawAgents(o.x, o.y, o.scale, glow, o.maxX, reveal);
    drawA(o.x, o.y, o.scale, glow, t, reveal);
    if (!reduce) {
      travelers(o.x, o.y, o.scale, t, Math.round(8 + ignite.value * 5), o.maxX);
      window.requestAnimationFrame(frame);
    }
  }

  window.addEventListener("resize", () => {
    box = layoutBox();
    if (reduce) {
      frame(0);
    }
  });
  frame(0);
}

const ether = document.querySelector("[data-ether]");
if (ether) {
  paintChamber(ether);
}
