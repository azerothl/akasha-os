/**
 * Spiral-galaxy point field — Abyssal force-field grammar, Akasha palette.
 * Seeds are a disk with arms + bulge + halo (not a uniform noise box).
 * Scroll blends neighbouring presets (forces + ramp).
 */
import * as THREE from "./vendor/three.module.min.js";

export const BACKGROUND = "#070b14";

function rgb(hex) {
  const n = parseInt(hex.replace("#", ""), 16);
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
}

/**
 * Presets reshape the same galaxy without dissolving it into static.
 * Keep amp/freq moderate so arms and core stay readable.
 */
export const PRESETS = [
  {
    id: "thesis",
    label: "Thesis",
    spread: 1,
    freq: 0.09,
    amp: 0.55,
    drift: 0.03,
    fall: 0.02,
    pull: 0.08,
    cluster: 0.12,
    cell: 2.4,
    swirl: 0.085,
    shear: 0.04,
    grain: 1.45,
    gain: 1.15,
    camZ: 11.4,
    mouseForce: 0.35,
    deep: rgb("#040a14"),
    mid: rgb("#0d4a58"),
    hot: rgb("#5ee7ff"),
  },
  {
    id: "caps",
    label: "Caps",
    spread: 0.92,
    freq: 0.12,
    amp: 0.4,
    drift: 0.04,
    fall: 0.01,
    pull: 0.42,
    cluster: 0.2,
    cell: 2.2,
    swirl: 0.12,
    shear: 0,
    grain: 1.25,
    gain: 1.2,
    camZ: 10.2,
    mouseForce: 0.28,
    deep: rgb("#05081c"),
    mid: rgb("#2b3d96"),
    hot: rgb("#9db9ff"),
  },
  {
    id: "surfaces",
    label: "Surfaces",
    spread: 1.05,
    freq: 0.11,
    amp: 0.42,
    drift: 0.04,
    fall: 0.015,
    pull: 0.05,
    cluster: 0.18,
    cell: 2.8,
    swirl: 0.14,
    shear: 0.02,
    grain: 1.5,
    gain: 1.22,
    camZ: 11.8,
    mouseForce: 0.45,
    deep: rgb("#02120f"),
    mid: rgb("#128a6a"),
    hot: rgb("#2ef0c8"),
  },
  {
    id: "local",
    label: "Local",
    spread: 0.88,
    freq: 0.11,
    amp: 0.38,
    drift: 0.035,
    fall: 0.01,
    pull: 0.72,
    cluster: 0.15,
    cell: 2.4,
    swirl: 0.22,
    shear: 0,
    grain: 1.35,
    gain: 1.3,
    camZ: 10.4,
    mouseForce: 0.32,
    deep: rgb("#0d0904"),
    mid: rgb("#9a6220"),
    hot: rgb("#ffdca8"),
  },
  {
    id: "bound",
    label: "Bound",
    spread: 1.02,
    freq: 0.14,
    amp: 0.85,
    drift: 0.08,
    fall: 0.03,
    pull: 0.1,
    cluster: 0.08,
    cell: 2.6,
    swirl: 0.05,
    shear: 0.55,
    grain: 1.2,
    gain: 1.18,
    camZ: 10.8,
    mouseForce: 0.55,
    deep: rgb("#160409"),
    mid: rgb("#9c2333"),
    hot: rgb("#ff5a48"),
  },
  {
    id: "path",
    label: "Path",
    spread: 1.06,
    freq: 0.08,
    amp: 0.62,
    drift: 0.03,
    fall: 0.02,
    pull: 0.12,
    cluster: 0.1,
    cell: 2.5,
    swirl: 0.06,
    shear: 0.12,
    grain: 1.4,
    gain: 1.1,
    camZ: 11.2,
    mouseForce: 0.3,
    deep: rgb("#020b10"),
    mid: rgb("#166a7c"),
    hot: rgb("#9ce6ff"),
  },
  {
    id: "install",
    label: "Install",
    spread: 1.12,
    freq: 0.07,
    amp: 0.7,
    drift: 0.025,
    fall: 0.015,
    pull: 0.06,
    cluster: 0.05,
    cell: 2.8,
    swirl: 0.04,
    shear: 0.18,
    grain: 1.15,
    gain: 0.95,
    camZ: 10.6,
    mouseForce: 0.22,
    deep: rgb("#070b14"),
    mid: rgb("#114b46"),
    hot: rgb("#2ef0c8"),
  },
];

export const SECTION_COUNT = PRESETS.length;

export const BLENDED_KEYS = [
  "spread",
  "freq",
  "amp",
  "drift",
  "fall",
  "pull",
  "cluster",
  "cell",
  "swirl",
  "shear",
  "grain",
  "gain",
  "camZ",
  "mouseForce",
];

export function makeBlendedField() {
  const out = {};
  for (const k of BLENDED_KEYS) out[k] = PRESETS[0][k];
  out.deep = [...PRESETS[0].deep];
  out.mid = [...PRESETS[0].mid];
  out.hot = [...PRESETS[0].hot];
  return out;
}

function lerp(a, b, t) {
  return a + (b - a) * t;
}

export function blendField(out, a, b, t) {
  for (const k of BLENDED_KEYS) out[k] = lerp(a[k], b[k], t);
  for (let i = 0; i < 3; i++) {
    out.deep[i] = lerp(a.deep[i], b.deep[i], t);
    out.mid[i] = lerp(a.mid[i], b.mid[i], t);
    out.hot[i] = lerp(a.hot[i], b.hot[i], t);
  }
}

export function cssRGB(c) {
  return `rgb(${Math.round(c[0] * 255)} ${Math.round(c[1] * 255)} ${Math.round(c[2] * 255)})`;
}

const SIMPLEX = /* glsl */ `
  vec3 mod289(vec3 x){ return x - floor(x * (1.0/289.0)) * 289.0; }
  vec4 mod289(vec4 x){ return x - floor(x * (1.0/289.0)) * 289.0; }
  vec4 permute(vec4 x){ return mod289(((x*34.0)+1.0)*x); }
  vec4 taylorInvSqrt(vec4 r){ return 1.79284291400159 - 0.85373472095314 * r; }
  float snoise(vec3 v){
    const vec2 C = vec2(1.0/6.0, 1.0/3.0);
    const vec4 D = vec4(0.0, 0.5, 1.0, 2.0);
    vec3 i  = floor(v + dot(v, C.yyy));
    vec3 x0 = v - i + dot(i, C.xxx);
    vec3 g = step(x0.yzx, x0.xyz);
    vec3 l = 1.0 - g;
    vec3 i1 = min(g.xyz, l.zxy);
    vec3 i2 = max(g.xyz, l.zxy);
    vec3 x1 = x0 - i1 + C.xxx;
    vec3 x2 = x0 - i2 + C.yyy;
    vec3 x3 = x0 - D.yyy;
    i = mod289(i);
    vec4 p = permute(permute(permute(
              i.z + vec4(0.0, i1.z, i2.z, 1.0))
            + i.y + vec4(0.0, i1.y, i2.y, 1.0))
            + i.x + vec4(0.0, i1.x, i2.x, 1.0));
    float n_ = 0.142857142857;
    vec3 ns = n_ * D.wyz - D.xzx;
    vec4 j = p - 49.0 * floor(p * ns.z * ns.z);
    vec4 x_ = floor(j * ns.z);
    vec4 y_ = floor(j - 7.0 * x_);
    vec4 x = x_ * ns.x + ns.yyyy;
    vec4 y = y_ * ns.x + ns.yyyy;
    vec4 h = 1.0 - abs(x) - abs(y);
    vec4 b0 = vec4(x.xy, y.xy);
    vec4 b1 = vec4(x.zw, y.zw);
    vec4 s0 = floor(b0) * 2.0 + 1.0;
    vec4 s1 = floor(b1) * 2.0 + 1.0;
    vec4 sh = -step(h, vec4(0.0));
    vec4 a0 = b0.xzyw + s0.xzyw * sh.xxyy;
    vec4 a1 = b1.xzyw + s1.xzyw * sh.zzww;
    vec3 p0 = vec3(a0.xy, h.x);
    vec3 p1 = vec3(a0.zw, h.y);
    vec3 p2 = vec3(a1.xy, h.z);
    vec3 p3 = vec3(a1.zw, h.w);
    vec4 norm = taylorInvSqrt(vec4(dot(p0,p0), dot(p1,p1), dot(p2,p2), dot(p3,p3)));
    p0 *= norm.x; p1 *= norm.y; p2 *= norm.z; p3 *= norm.w;
    vec4 mm = max(0.6 - vec4(dot(x0,x0), dot(x1,x1), dot(x2,x2), dot(x3,x3)), 0.0);
    mm = mm * mm;
    return 42.0 * dot(mm*mm, vec4(dot(p0,x0), dot(p1,x1), dot(p2,x2), dot(p3,x3)));
  }
`;

const RAMP = /* glsl */ `
  vec3 ramp(vec3 deep, vec3 mid, vec3 hot, float t) {
    float k = clamp(t, 0.0, 1.0);
    return k < 0.5 ? mix(deep, mid, k * 2.0) : mix(mid, hot, (k - 0.5) * 2.0);
  }
`;

const fieldVertexShader = /* glsl */ `
  uniform float uTime;
  uniform float uSize;
  uniform float uPixelRatio;
  uniform float uIntro;
  uniform float uVelocity;
  uniform vec3  uMouse;
  uniform float uMouseForce;
  uniform float uSpread;
  uniform float uFreq;
  uniform float uAmp;
  uniform float uDrift;
  uniform float uFall;
  uniform float uPull;
  uniform float uCluster;
  uniform float uCell;
  uniform float uSwirl;
  uniform float uShear;
  uniform float uGrain;
  uniform float uGain;
  uniform vec3 uDeep;
  uniform vec3 uMid;
  uniform vec3 uHot;
  attribute vec3  aSeed;
  attribute vec3  aRand;
  attribute float aTone;
  attribute float aScale;
  varying vec3  vColor;
  varying float vAlpha;
  ${SIMPLEX}
  ${RAMP}
  vec3 hash3(vec3 c) {
    vec3 p = vec3(
      dot(c, vec3(127.1, 311.7,  74.7)),
      dot(c, vec3(269.5, 183.3, 246.1)),
      dot(c, vec3(113.5, 271.9, 124.6))
    );
    return fract(sin(p) * 43758.5453123);
  }
  vec2 rot(vec2 p, float a) {
    float c = cos(a), s = sin(a);
    return vec2(c * p.x - s * p.y, s * p.x + c * p.y);
  }
  void main() {
    vec3 p = aSeed;

    /* Slow vertical breathing — not marine-snow recycle. */
    p.y += sin(uTime * 0.07 + aRand.z * 6.283) * uFall * 0.55;

    p *= uSpread;

    if (uCluster > 0.001) {
      vec3 cellIdx = floor(p / uCell);
      vec3 centre = (cellIdx + 0.5) * uCell + (hash3(cellIdx) - 0.5) * uCell * 0.45;
      p = mix(p, centre + (p - centre) * 0.35, uCluster);
    }

    if (uPull > 0.001) {
      vec3 dir = normalize(p + vec3(0.0001));
      float r = 0.35 + pow(aRand.x, 1.6) * 2.4;
      p = mix(p, dir * r, uPull);
    }

    /* Low-frequency warp only — keeps arms readable. */
    vec3 q = p * uFreq + vec3(0.0, 0.0, uTime * uDrift);
    vec3 warp = vec3(snoise(q), snoise(q + 19.7), snoise(q + 43.3));
    p += warp * uAmp;

    /* Differential rotation: core turns faster → spiral feels alive. */
    if (abs(uSwirl) > 0.001) {
      float rr = length(p.xy);
      p.xy = rot(p.xy, uSwirl * uTime / (0.35 + rr * 0.55));
    }

    if (abs(uShear) > 0.001) {
      p.x += sin(p.y * 0.35 + uTime * 0.22) * uShear * 0.9;
      p.z += cos(p.y * 0.28 - uTime * 0.16) * uShear * 0.35;
    }

    vec2 toMouse = p.xy - uMouse.xy;
    float md = length(toMouse);
    float influence = smoothstep(2.6, 0.0, md);
    p.xy += normalize(toMouse + vec2(0.0001)) * influence * uMouseForce * 0.9;

    p *= 1.0 + (1.0 - uIntro) * 1.35;

    vec4 mv = modelViewMatrix * vec4(p, 1.0);
    gl_Position = projectionMatrix * mv;

    float radial = length(aSeed.xy);
    float coreBoost = smoothstep(6.5, 0.4, radial);
    float energy = clamp(length(warp) * 0.35 + coreBoost * 0.45, 0.0, 1.0);
    float tone = clamp(aTone * 0.55 + energy * 0.55, 0.0, 1.0);
    vColor = ramp(uDeep, uMid, uHot, tone) * uGain;

    float sizePx = uSize * uGrain * aScale * uPixelRatio * (52.0 / max(0.001, -mv.z));
    sizePx *= 0.75 + coreBoost * 0.9;
    gl_PointSize = max(sizePx, uPixelRatio * 1.15);

    float shrink = clamp(sizePx / (uPixelRatio * 1.15), 0.4, 1.0);
    /* Gentle shimmer — not TV-static flicker. */
    float shimmer = 0.88 + 0.12 * sin(uTime * (0.35 + aRand.y * 0.55) + aRand.z * 6.283);
    float depthFade = smoothstep(-28.0, -1.5, mv.z);
    float armWeight = mix(0.55, 1.0, aTone);

    vAlpha = shimmer * depthFade * shrink * uIntro * armWeight * (1.0 + uVelocity * 0.2);
  }
`;

const fieldFragmentShader = /* glsl */ `
  varying vec3  vColor;
  varying float vAlpha;
  void main() {
    vec2 uv = gl_PointCoord - 0.5;
    float d = length(uv);
    if (d > 0.5) discard;
    float f = 1.0 - d * 2.0;
    /* Soft star + faint nebula halo — not hard grain pixels. */
    float halo = pow(f, 1.15) * 0.72;
    float core = pow(f, 3.6);
    float spark = pow(f, 9.0) * 0.65;
    float shape = halo + core + spark;
    gl_FragColor = vec4(vColor * (halo * 0.85 + core * 1.25 + spark * 1.5), shape * vAlpha);
  }
`;

function mulberry32(a) {
  return function () {
    let t = (a += 0x6d2b79f5);
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function damp(current, target, lambda, dt) {
  return current + (target - current) * (1 - Math.exp(-lambda * dt));
}

function gauss(rnd) {
  /* Box-Muller */
  let u = 0;
  let v = 0;
  while (u === 0) u = rnd();
  while (v === 0) v = rnd();
  return Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v);
}

/**
 * Build a face-on spiral galaxy: bulge + logarithmic arms + thin disk + sparse halo.
 * Returns { seed, rand, tone, scale } typed arrays.
 */
function buildGalaxySeeds(count, rnd) {
  const seed = new Float32Array(count * 3);
  const rand = new Float32Array(count * 3);
  const tone = new Float32Array(count);
  const scale = new Float32Array(count);

  const ARMS = 3;
  const ARM_TIGHT = 0.38;
  const DISK_R = 7.8;

  for (let i = 0; i < count; i++) {
    const i3 = i * 3;
    const roll = rnd();
    let x;
    let y;
    let z;
    let t;
    let s;

    if (roll < 0.18) {
      /* Central bulge — dense, bright. */
      const r = Math.abs(gauss(rnd)) * 1.15;
      const a = rnd() * Math.PI * 2;
      x = Math.cos(a) * r;
      y = Math.sin(a) * r * 0.92;
      z = gauss(rnd) * 0.55;
      t = 0.72 + rnd() * 0.28;
      s = 1.1 + Math.pow(rnd(), 2.2) * 2.8;
    } else if (roll < 0.82) {
      /* Spiral arms. */
      const arm = Math.floor(rnd() * ARMS);
      const u = Math.pow(rnd(), 0.65);
      const radius = 0.55 + u * DISK_R;
      const theta =
        arm * ((Math.PI * 2) / ARMS) +
        radius * ARM_TIGHT +
        (rnd() - 0.5) * 0.55;
      const jitter = (rnd() - 0.5) * (0.35 + radius * 0.08);
      const jr = (rnd() - 0.5) * 0.55;
      x = Math.cos(theta) * (radius + jr) + Math.cos(theta + 1.2) * jitter;
      y = Math.sin(theta) * (radius + jr) + Math.sin(theta + 1.2) * jitter;
      z = gauss(rnd) * (0.18 + radius * 0.04);
      t = 0.35 + (1 - u) * 0.45 + rnd() * 0.2;
      s = 0.55 + Math.pow(rnd(), 2.6) * 2.4 + (1 - u) * 0.5;
    } else if (roll < 0.94) {
      /* Inter-arm disk dust — sparse so voids read. */
      const u = Math.pow(rnd(), 0.85);
      const radius = 1.2 + u * DISK_R * 0.95;
      const a = rnd() * Math.PI * 2;
      x = Math.cos(a) * radius;
      y = Math.sin(a) * radius;
      z = gauss(rnd) * 0.22;
      t = 0.12 + rnd() * 0.25;
      s = 0.35 + Math.pow(rnd(), 3.4) * 1.1;
    } else {
      /* Distant halo stars. */
      const r = 4.5 + Math.pow(rnd(), 0.5) * 7.5;
      const a = rnd() * Math.PI * 2;
      const elev = (rnd() - 0.5) * Math.PI * 0.55;
      x = Math.cos(a) * Math.cos(elev) * r;
      y = Math.sin(a) * Math.cos(elev) * r;
      z = Math.sin(elev) * r * 0.65;
      t = 0.55 + rnd() * 0.45;
      s = 0.7 + Math.pow(rnd(), 4) * 3.2;
    }

    seed[i3] = x;
    seed[i3 + 1] = y;
    seed[i3 + 2] = z;
    rand[i3] = rnd();
    rand[i3 + 1] = rnd();
    rand[i3 + 2] = rnd();
    tone[i] = t;
    scale[i] = s;
  }

  return { seed, rand, tone, scale };
}

function pickCount(width, reduced) {
  /* Fewer, larger stars read as a galaxy; dense dust reads as noise. */
  if (reduced) return 12000;
  if (width < 700) return 22000;
  if (width < 1100) return 36000;
  if (width < 1700) return 48000;
  return 58000;
}

/**
 * Mount the WebGL chamber into `canvas`. Reads/writes shared `view`.
 */
export function mountField(canvas, view) {
  const reduced = view.reduced;
  const count = pickCount(window.innerWidth, reduced);
  const renderer = new THREE.WebGLRenderer({
    canvas,
    antialias: false,
    alpha: false,
    powerPreference: "high-performance",
    stencil: false,
    depth: false,
  });
  renderer.setClearColor(BACKGROUND, 1);
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, reduced ? 1 : 1.6));
  renderer.toneMapping = THREE.NoToneMapping;

  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(42, 1, 0.1, 200);
  /* Slight tilt so the disk reads in depth, not as flat static. */
  camera.position.set(0, -1.8, PRESETS[0].camZ);
  camera.lookAt(0, 0, 0);

  const rnd = mulberry32(0x51a2c7e3);
  const { seed, rand, tone, scale } = buildGalaxySeeds(count, rnd);

  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(new Float32Array(count * 3), 3));
  geometry.setAttribute("aSeed", new THREE.BufferAttribute(seed, 3));
  geometry.setAttribute("aRand", new THREE.BufferAttribute(rand, 3));
  geometry.setAttribute("aTone", new THREE.BufferAttribute(tone, 1));
  geometry.setAttribute("aScale", new THREE.BufferAttribute(scale, 1));
  geometry.boundingSphere = new THREE.Sphere(new THREE.Vector3(), 90);

  const material = new THREE.ShaderMaterial({
    vertexShader: fieldVertexShader,
    fragmentShader: fieldFragmentShader,
    transparent: true,
    depthWrite: false,
    depthTest: false,
    blending: THREE.AdditiveBlending,
    uniforms: {
      uTime: { value: 0 },
      uSize: { value: 1.15 },
      uPixelRatio: { value: renderer.getPixelRatio() },
      uIntro: { value: 0 },
      uVelocity: { value: 0 },
      uMouse: { value: new THREE.Vector3(999, 999, 0) },
      uMouseForce: { value: 0 },
      uSpread: { value: 1 },
      uFreq: { value: 0.09 },
      uAmp: { value: 0.55 },
      uDrift: { value: 0.03 },
      uFall: { value: 0.02 },
      uPull: { value: 0.08 },
      uCluster: { value: 0.12 },
      uCell: { value: 2.4 },
      uSwirl: { value: 0.085 },
      uShear: { value: 0.04 },
      uGrain: { value: 1.45 },
      uGain: { value: 1.15 },
      uDeep: { value: new THREE.Color() },
      uMid: { value: new THREE.Color() },
      uHot: { value: new THREE.Color() },
    },
  });

  const points = new THREE.Points(geometry, material);
  /* Tip the galaxy disk toward the camera. */
  points.rotation.x = -0.55;
  points.rotation.z = 0.35;
  scene.add(points);

  const clock = new THREE.Clock();
  let raf = 0;
  let disposed = false;

  function resize() {
    const w = window.innerWidth;
    const h = window.innerHeight;
    renderer.setSize(w, h, false);
    camera.aspect = w / Math.max(h, 1);
    camera.updateProjectionMatrix();
    material.uniforms.uPixelRatio.value = renderer.getPixelRatio();
  }

  function frame() {
    if (disposed) return;
    raf = requestAnimationFrame(frame);
    const dt = Math.min(clock.getDelta(), 1 / 30);
    view.smoothX = damp(view.smoothX, view.pointerX, 4.5, dt);
    view.smoothY = damp(view.smoothY, view.pointerY, 4.5, dt);
    view.velocity = damp(view.velocity, 0, 3.5, dt);

    const f = view.field;
    const u = material.uniforms;
    u.uTime.value = clock.elapsedTime;
    u.uIntro.value = view.intro;
    u.uVelocity.value = view.velocity;
    u.uSpread.value = f.spread;
    u.uFreq.value = f.freq;
    u.uAmp.value = f.amp;
    u.uDrift.value = f.drift;
    u.uFall.value = f.fall;
    u.uPull.value = f.pull;
    u.uCluster.value = f.cluster;
    u.uCell.value = f.cell;
    u.uSwirl.value = f.swirl;
    u.uShear.value = f.shear;
    u.uGrain.value = f.grain;
    u.uGain.value = f.gain;
    u.uMouseForce.value = f.mouseForce;
    u.uDeep.value.setRGB(f.deep[0], f.deep[1], f.deep[2]);
    u.uMid.value.setRGB(f.mid[0], f.mid[1], f.mid[2]);
    u.uHot.value.setRGB(f.hot[0], f.hot[1], f.hot[2]);
    u.uMouse.value.set(view.smoothX * 3.6, view.smoothY * 2.2, 0);

    const targetZ = f.camZ;
    camera.position.z = damp(camera.position.z, targetZ, 2.2, dt);
    camera.position.y = damp(camera.position.y, -1.8 + view.smoothY * 0.25, 2.0, dt);
    camera.lookAt(0, 0, 0);

    /* Slow whole-galaxy turn so arms stay legible. */
    points.rotation.z = 0.35 + clock.elapsedTime * 0.012;

    renderer.render(scene, camera);
  }

  resize();
  window.addEventListener("resize", resize);
  if (!reduced) {
    frame();
  } else {
    view.intro = 1;
    material.uniforms.uIntro.value = 1;
    const f = view.field;
    const u = material.uniforms;
    u.uSpread.value = f.spread;
    u.uFreq.value = f.freq;
    u.uAmp.value = f.amp;
    u.uDrift.value = f.drift;
    u.uFall.value = f.fall;
    u.uPull.value = f.pull;
    u.uCluster.value = f.cluster;
    u.uCell.value = f.cell;
    u.uSwirl.value = f.swirl;
    u.uShear.value = f.shear;
    u.uGrain.value = f.grain;
    u.uGain.value = f.gain;
    u.uDeep.value.setRGB(f.deep[0], f.deep[1], f.deep[2]);
    u.uMid.value.setRGB(f.mid[0], f.mid[1], f.mid[2]);
    u.uHot.value.setRGB(f.hot[0], f.hot[1], f.hot[2]);
    camera.position.z = f.camZ;
    camera.lookAt(0, 0, 0);
    renderer.render(scene, camera);
  }

  return {
    dispose() {
      disposed = true;
      cancelAnimationFrame(raf);
      window.removeEventListener("resize", resize);
      geometry.dispose();
      material.dispose();
      renderer.dispose();
    },
  };
}
