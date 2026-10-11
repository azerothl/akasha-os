/**
 * Cloud-chamber point field — Abyssal force-field grammar, Akasha palette.
 * One seed cloud; scroll blends neighbouring presets (forces + ramp).
 */
import * as THREE from "./vendor/three.module.min.js";

export const BACKGROUND = "#070b14";

function rgb(hex) {
  const n = parseInt(hex.replace("#", ""), 16);
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
}

export const PRESETS = [
  {
    id: "thesis",
    label: "Thesis",
    spread: 1,
    freq: 0.15,
    amp: 1.5,
    drift: 0.05,
    fall: 0.62,
    pull: 0,
    cluster: 0,
    cell: 3.2,
    swirl: 0.02,
    shear: 0,
    grain: 1,
    gain: 1,
    camZ: 10.2,
    mouseForce: 0.5,
    deep: rgb("#05141a"),
    mid: rgb("#126b62"),
    hot: rgb("#5ee7ff"),
  },
  {
    id: "caps",
    label: "Caps",
    spread: 0.94,
    freq: 0.26,
    amp: 0.95,
    drift: 0.09,
    fall: 0.12,
    pull: 0.58,
    cluster: 0,
    cell: 3.2,
    swirl: 0.1,
    shear: 0,
    grain: 0.86,
    gain: 1.05,
    camZ: 9.1,
    mouseForce: 0.34,
    deep: rgb("#05081c"),
    mid: rgb("#2b3d96"),
    hot: rgb("#9db9ff"),
  },
  {
    id: "surfaces",
    label: "Surfaces",
    spread: 1.08,
    freq: 0.52,
    amp: 0.42,
    drift: 0.14,
    fall: 0.06,
    pull: 0,
    cluster: 0.88,
    cell: 3.05,
    swirl: 0.26,
    shear: 0,
    grain: 1.12,
    gain: 1.15,
    camZ: 10.6,
    mouseForce: 0.72,
    deep: rgb("#02120f"),
    mid: rgb("#128a6a"),
    hot: rgb("#2ef0c8"),
  },
  {
    id: "local",
    label: "Local",
    spread: 1,
    freq: 0.34,
    amp: 0.66,
    drift: 0.1,
    fall: 0.05,
    pull: 0.92,
    cluster: 0,
    cell: 3.2,
    swirl: 0.34,
    shear: 0,
    grain: 1,
    gain: 1.2,
    camZ: 9.4,
    mouseForce: 0.5,
    deep: rgb("#0d0904"),
    mid: rgb("#9a6220"),
    hot: rgb("#ffdca8"),
  },
  {
    id: "bound",
    label: "Bound",
    spread: 1.05,
    freq: 0.31,
    amp: 1.85,
    drift: 0.26,
    fall: 0.18,
    pull: 0.12,
    cluster: 0,
    cell: 3.2,
    swirl: 0.06,
    shear: 0.92,
    grain: 0.9,
    gain: 1.1,
    camZ: 9.8,
    mouseForce: 0.85,
    deep: rgb("#160409"),
    mid: rgb("#9c2333"),
    hot: rgb("#ff5a48"),
  },
  {
    id: "path",
    label: "Path",
    spread: 1.02,
    freq: 0.1,
    amp: 1.15,
    drift: 0.045,
    fall: 0.2,
    pull: 0.16,
    cluster: 0,
    cell: 3.2,
    swirl: 0.05,
    shear: 0.26,
    grain: 1.05,
    gain: 1,
    camZ: 10,
    mouseForce: 0.45,
    deep: rgb("#020b10"),
    mid: rgb("#166a7c"),
    hot: rgb("#9ce6ff"),
  },
  {
    id: "install",
    label: "Install",
    spread: 1.15,
    freq: 0.08,
    amp: 1.4,
    drift: 0.05,
    fall: 0.1,
    pull: 0.1,
    cluster: 0,
    cell: 3.2,
    swirl: 0.03,
    shear: 0.4,
    grain: 0.8,
    gain: 0.85,
    camZ: 9.6,
    mouseForce: 0.3,
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
  const float BAND = 8.0;
  void main() {
    vec3 p = aSeed;
    p.y = mod(p.y - uTime * uFall + BAND, BAND * 2.0) - BAND;
    p *= uSpread;
    if (uCluster > 0.001) {
      vec3 cellIdx = floor(p / uCell);
      vec3 centre = (cellIdx + 0.5) * uCell + (hash3(cellIdx) - 0.5) * uCell * 0.62;
      p = mix(p, centre + (p - centre) * 0.26, uCluster);
    }
    if (uPull > 0.001) {
      vec3 dir = normalize(p + vec3(0.0001));
      float r = 0.45 + pow(aRand.x, 1.9) * 3.05;
      p = mix(p, dir * r, uPull);
    }
    vec3 q = p * uFreq + vec3(0.0, 0.0, uTime * uDrift);
    vec3 warp = vec3(snoise(q), snoise(q + 19.7), snoise(q + 43.3));
    vec3 q2 = q * 2.35 + 7.3;
    warp += vec3(snoise(q2), snoise(q2 + 19.7), snoise(q2 + 43.3)) * 0.42;
    p += warp * uAmp;
    if (abs(uSwirl) > 0.001) {
      float rr = length(p.xy);
      p.xy = rot(p.xy, uSwirl * uTime / (0.55 + rr * 0.4));
    }
    if (abs(uShear) > 0.001) {
      p.x += sin(p.y * 0.52 + uTime * 0.34) * uShear * 1.7;
      p.z += cos(p.y * 0.41 - uTime * 0.22) * uShear * 0.6;
    }
    vec2 toMouse = p.xy - uMouse.xy;
    float md = length(toMouse);
    float influence = smoothstep(3.0, 0.0, md);
    p.xy += normalize(toMouse + vec2(0.0001)) * influence * uMouseForce * 1.3;
    p *= 1.0 + (1.0 - uIntro) * 1.8;
    vec4 mv = modelViewMatrix * vec4(p, 1.0);
    gl_Position = projectionMatrix * mv;
    float energy = clamp(length(warp) * 0.42, 0.0, 1.0);
    float tone = clamp(aTone * 0.72 + energy * 0.5, 0.0, 1.0);
    vColor = ramp(uDeep, uMid, uHot, tone) * uGain;
    float sizePx = uSize * uGrain * aScale * uPixelRatio * (44.0 / max(0.001, -mv.z));
    gl_PointSize = max(sizePx, uPixelRatio * 0.9);
    float shrink = clamp(sizePx / (uPixelRatio * 0.9), 0.35, 1.0);
    float flicker = 0.68 + 0.32 * sin(uTime * (0.9 + aRand.y * 2.2) + aRand.z * 12.566);
    float depthFade = smoothstep(-30.0, -2.0, mv.z);
    vAlpha = flicker * depthFade * shrink * uIntro * (1.0 + uVelocity * 0.35);
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
    float halo = pow(f, 1.8) * 0.45;
    float core = pow(f, 6.5);
    gl_FragColor = vec4(vColor * (halo + core * 1.85), (halo + core) * vAlpha);
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

function pickCount(width, reduced) {
  if (reduced) return 18000;
  if (width < 700) return 28000;
  if (width < 1100) return 48000;
  if (width < 1700) return 72000;
  return 90000;
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
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, reduced ? 1 : 1.75));
  renderer.toneMapping = THREE.NoToneMapping;

  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(45, 1, 0.1, 200);
  camera.position.set(0, 0, PRESETS[0].camZ);

  const BOX = [11, 8, 5];
  const rnd = mulberry32(0x428a2f98);
  const seed = new Float32Array(count * 3);
  const rand = new Float32Array(count * 3);
  const tone = new Float32Array(count);
  const scale = new Float32Array(count);

  for (let i = 0; i < count; i++) {
    const i3 = i * 3;
    seed[i3] = (rnd() - 0.5) * 2 * BOX[0];
    seed[i3 + 1] = (rnd() - 0.5) * 2 * BOX[1];
    seed[i3 + 2] = (rnd() - 0.5) * 2 * BOX[2];
    rand[i3] = rnd();
    rand[i3 + 1] = rnd();
    rand[i3 + 2] = rnd();
    tone[i] = rnd();
    scale[i] = 0.42 + Math.pow(rnd(), 3.1) * 2.2;
  }

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
      uSize: { value: 1 },
      uPixelRatio: { value: renderer.getPixelRatio() },
      uIntro: { value: 0 },
      uVelocity: { value: 0 },
      uMouse: { value: new THREE.Vector3(999, 999, 0) },
      uMouseForce: { value: 0 },
      uSpread: { value: 1 },
      uFreq: { value: 0.15 },
      uAmp: { value: 1.5 },
      uDrift: { value: 0.05 },
      uFall: { value: 0.6 },
      uPull: { value: 0 },
      uCluster: { value: 0 },
      uCell: { value: 3.2 },
      uSwirl: { value: 0 },
      uShear: { value: 0 },
      uGrain: { value: 1 },
      uGain: { value: 1 },
      uDeep: { value: new THREE.Color() },
      uMid: { value: new THREE.Color() },
      uHot: { value: new THREE.Color() },
    },
  });

  const points = new THREE.Points(geometry, material);
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
    u.uMouse.value.set(view.smoothX * 4.2, view.smoothY * 2.6, 0);
    camera.position.z = damp(camera.position.z, f.camZ, 2.2, dt);

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
