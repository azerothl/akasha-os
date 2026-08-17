import * as THREE from "./vendor/three.module.min.js";

const VOID = 0x070b14;
const SIGNAL = 0x3ee0c4;
const PAPER = 0xdce6f0;

const bodies = [
  { id: "MEMORY", orbit: 0.32, size: 0.055, speed: 0.62, phase: 0.4, kind: "memory" },
  { id: "CAPS", orbit: 0.56, size: 0.068, speed: 0.41, phase: 1.7, kind: "caps" },
  { id: "GPU", orbit: 0.68, size: 0.078, speed: 0.27, phase: 3.3, kind: "gpu" },
  { id: "AGENTS", orbit: 0.8, size: 0.092, speed: 0.16, phase: 5.1, kind: "agents" },
];

const rails = [0.2, 0.32, 0.44, 0.56, 0.68, 0.8, 0.92];

const cogLayout = [
  { x: 0, z: 0, r: 0.16, teeth: 18, speed: 0.18, y: -0.08 },
  { x: 0.26, z: 0.14, r: 0.12, teeth: 14, speed: -0.28, y: -0.072 },
  { x: -0.24, z: 0.18, r: 0.13, teeth: 15, speed: 0.24, y: -0.075 },
  { x: 0.1, z: -0.28, r: 0.11, teeth: 12, speed: -0.32, y: -0.07 },
  { x: -0.3, z: -0.12, r: 0.1, teeth: 12, speed: 0.3, y: -0.068 },
  { x: 0.38, z: -0.16, r: 0.12, teeth: 14, speed: -0.22, y: -0.074 },
  { x: -0.14, z: 0.36, r: 0.09, teeth: 10, speed: 0.36, y: -0.066 },
  { x: 0.2, z: 0.34, r: 0.08, teeth: 9, speed: -0.4, y: -0.064 },
  { x: -0.4, z: 0.04, r: 0.11, teeth: 13, speed: 0.26, y: -0.07 },
  { x: 0.02, z: 0.46, r: 0.08, teeth: 9, speed: -0.34, y: -0.062 },
];

function gearGeometry(radius, teeth, depth) {
  const shape = new THREE.Shape();
  const outer = radius;
  const valley = radius * 0.86;
  const steps = teeth * 2;
  for (let i = 0; i <= steps; i += 1) {
    const a = (i / steps) * Math.PI * 2;
    const r = i % 2 === 0 ? outer : valley;
    const x = Math.cos(a) * r;
    const y = Math.sin(a) * r;
    if (i === 0) {
      shape.moveTo(x, y);
    } else {
      shape.lineTo(x, y);
    }
  }
  const hole = new THREE.Path();
  hole.absarc(0, 0, radius * 0.38, 0, Math.PI * 2, false);
  shape.holes.push(hole);
  const geo = new THREE.ExtrudeGeometry(shape, {
    depth,
    bevelEnabled: true,
    bevelThickness: depth * 0.18,
    bevelSize: radius * 0.03,
    bevelSegments: 1,
    curveSegments: 1,
  });
  geo.rotateX(-Math.PI / 2);
  geo.center();
  return geo;
}

function globeMaterial(kind) {
  if (kind === "memory") {
    return new THREE.MeshStandardMaterial({
      color: PAPER,
      roughness: 0.42,
      metalness: 0.12,
    });
  }
  if (kind === "caps") {
    return new THREE.MeshStandardMaterial({
      color: SIGNAL,
      emissive: SIGNAL,
      emissiveIntensity: 1.4,
      roughness: 0.22,
      metalness: 0.2,
    });
  }
  if (kind === "gpu") {
    return new THREE.MeshStandardMaterial({
      color: 0x101820,
      roughness: 0.18,
      metalness: 0.85,
    });
  }
  return new THREE.MeshStandardMaterial({
    color: PAPER,
    emissive: SIGNAL,
    emissiveIntensity: 0.18,
    roughness: 0.35,
    metalness: 0.08,
  });
}

function makeLabel(text) {
  const canvas = document.createElement("canvas");
  canvas.width = 512;
  canvas.height = 128;
  const ctx = canvas.getContext("2d");
  ctx.clearRect(0, 0, 512, 128);
  ctx.font = '700 52px "Archivo Narrow", "Arial Narrow", sans-serif';
  ctx.textAlign = "left";
  ctx.textBaseline = "middle";
  ctx.shadowColor = "#06343f";
  ctx.shadowBlur = 28;
  ctx.fillStyle = "#0a5c68";
  ctx.fillText(text, 36, 64);
  ctx.shadowColor = "#0e7a86";
  ctx.shadowBlur = 14;
  ctx.fillStyle = "#12808c";
  ctx.fillText(text, 36, 64);
  ctx.shadowBlur = 0;
  ctx.fillStyle = "#dce6f0";
  ctx.fillText(text, 36, 64);
  const map = new THREE.CanvasTexture(canvas);
  map.colorSpace = THREE.SRGBColorSpace;
  const sprite = new THREE.Sprite(
    new THREE.SpriteMaterial({ map, transparent: true, depthTest: false }),
  );
  sprite.scale.set(0.34, 0.085, 1);
  sprite.center.set(0, 0.5);
  return sprite;
}

function paintOrrery(canvas) {
  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const plate = document.body.classList.contains("plate");
  const lost = Boolean(document.querySelector("main.lost"));

  let renderer;
  try {
    renderer = new THREE.WebGLRenderer({
      canvas,
      antialias: true,
      alpha: false,
      powerPreference: "high-performance",
    });
  } catch {
    return;
  }

  renderer.setClearColor(VOID, 1);
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.12;
  renderer.shadowMap.enabled = true;
  renderer.shadowMap.type = THREE.PCFSoftShadowMap;

  const scene = new THREE.Scene();
  scene.fog = new THREE.Fog(VOID, 4.2, 9.5);

  const camera = new THREE.PerspectiveCamera(28, 1, 0.08, 24);
  const machine = new THREE.Group();
  scene.add(machine);

  scene.add(new THREE.AmbientLight(0x1c2733, 0.55));
  const hemi = new THREE.HemisphereLight(SIGNAL, VOID, 0.35);
  scene.add(hemi);
  const keyLight = new THREE.DirectionalLight(0xe8f0f6, 1.15);
  keyLight.position.set(1.4, 2.6, 1.8);
  keyLight.castShadow = true;
  keyLight.shadow.mapSize.set(1024, 1024);
  keyLight.shadow.camera.near = 0.5;
  keyLight.shadow.camera.far = 8;
  scene.add(keyLight);
  const cyanFill = new THREE.PointLight(SIGNAL, 3.2, 5.5, 1.4);
  cyanFill.position.set(0, 0.28, 0);
  scene.add(cyanFill);

  const metal = new THREE.MeshStandardMaterial({
    color: 0x1a2430,
    roughness: 0.38,
    metalness: 0.82,
  });
  const metalRim = new THREE.MeshStandardMaterial({
    color: 0x2a3948,
    roughness: 0.32,
    metalness: 0.88,
    emissive: SIGNAL,
    emissiveIntensity: 0.08,
  });

  const deck = new THREE.Mesh(new THREE.CylinderGeometry(1.02, 1.02, 0.04, 80), metal);
  deck.position.y = -0.09;
  deck.receiveShadow = true;
  machine.add(deck);

  const cogs = cogLayout.map((spec) => {
    const mesh = new THREE.Mesh(gearGeometry(spec.r, spec.teeth, 0.045), metalRim);
    mesh.position.set(spec.x, spec.y, spec.z);
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    machine.add(mesh);
    return { mesh, speed: spec.speed };
  });

  rails.forEach((radius, index) => {
    const track = new THREE.Mesh(
      new THREE.TorusGeometry(radius, 0.01, 10, 96),
      new THREE.MeshStandardMaterial({
        color: 0x8aa0b0,
        roughness: 0.28,
        metalness: 0.9,
      }),
    );
    track.rotation.x = Math.PI / 2;
    track.position.y = -0.03 - index * 0.002;
    machine.add(track);

    const glow = new THREE.Mesh(
      new THREE.TorusGeometry(radius, 0.007, 8, 96),
      new THREE.MeshStandardMaterial({
        color: SIGNAL,
        emissive: SIGNAL,
        emissiveIntensity: 2.2,
        roughness: 0.2,
        metalness: 0.1,
        toneMapped: false,
      }),
    );
    glow.rotation.x = Math.PI / 2;
    glow.position.y = 0.018;
    machine.add(glow);
  });

  for (let i = 0; i < 6; i += 1) {
    const ring = new THREE.Mesh(
      new THREE.TorusGeometry(0.05 + i * 0.014, 0.006, 8, 40),
      new THREE.MeshStandardMaterial({
        color: SIGNAL,
        emissive: SIGNAL,
        emissiveIntensity: 0.6 + i * 0.12,
        metalness: 0.4,
        roughness: 0.3,
      }),
    );
    ring.rotation.x = Math.PI / 2;
    ring.position.y = 0.01 + i * 0.012;
    machine.add(ring);
  }

  const hubCore = new THREE.Mesh(
    new THREE.SphereGeometry(0.028, 24, 16),
    new THREE.MeshStandardMaterial({
      color: SIGNAL,
      emissive: SIGNAL,
      emissiveIntensity: 2.4,
      toneMapped: false,
    }),
  );
  hubCore.position.y = 0.03;
  machine.add(hubCore);

  const planets = bodies.map((body) => {
    const group = new THREE.Group();
    const sphere = new THREE.Mesh(new THREE.SphereGeometry(body.size, 32, 24), globeMaterial(body.kind));
    sphere.castShadow = true;
    group.add(sphere);
    [1.22, 1.38].forEach((scale, i) => {
      const gimbal = new THREE.Mesh(
        new THREE.TorusGeometry(body.size * scale, 0.0045, 8, 32),
        new THREE.MeshStandardMaterial({
          color: SIGNAL,
          emissive: SIGNAL,
          emissiveIntensity: 0.7,
          metalness: 0.6,
          roughness: 0.28,
        }),
      );
      gimbal.rotation.x = i === 0 ? Math.PI / 2 : 0.4;
      gimbal.rotation.y = i === 0 ? 0 : 0.7;
      group.add(gimbal);
    });
    const armGeo = new THREE.CylinderGeometry(0.006, 0.006, 1, 8);
    const arm = new THREE.Mesh(armGeo, metalRim);
    arm.castShadow = true;
    machine.add(arm);
    const label = makeLabel(body.id);
    group.add(label);
    label.position.set(body.size + 0.06, 0.02, 0);
    machine.add(group);
    return { body, group, arm };
  });

  const bearings = rails.slice(0, 6).map((orbit, i) => {
    const bead = new THREE.Mesh(
      new THREE.SphereGeometry(0.016, 16, 12),
      new THREE.MeshStandardMaterial({ color: 0x9aa8b4, metalness: 0.9, roughness: 0.22 }),
    );
    machine.add(bead);
    return { bead, orbit, phase: i * 1.1 };
  });

  const wind = { value: 1, target: 1 };
  const key = document.querySelector(".key");
  if (key && !reduce) {
    key.addEventListener("pointerenter", () => {
      wind.target = 3.4;
    });
    key.addEventListener("pointerleave", () => {
      wind.target = 1;
    });
    key.addEventListener("focus", () => {
      wind.target = 3.4;
    });
    key.addEventListener("blur", () => {
      wind.target = 1;
    });
  }

  function layout() {
    const w = Math.max(1, canvas.clientWidth);
    const h = Math.max(1, canvas.clientHeight);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
    if (plate) {
      machine.position.set(1.15, 0.62, 0);
      machine.scale.setScalar(0.42);
      camera.position.set(0.15, 1.7, 2.6);
    } else if (lost) {
      machine.position.set(0, 0, 0);
      machine.scale.setScalar(1);
      camera.position.set(0.2, 1.55, 2.5);
    } else {
      machine.position.set(-1.18, -0.08, 0);
      machine.scale.setScalar(0.62);
      camera.position.set(0.08, 1.28, 2.35);
    }
    camera.lookAt(machine.position.x, machine.position.y * 0.2, 0);
  }

  let t = 1.15;
  let last = 0;
  let frame = 0;

  function pose(now) {
    const dt = last ? Math.min(0.05, (now - last) / 1000) : 0.016;
    last = now;
    if (!reduce) {
      t += dt * wind.value;
      wind.value += (wind.target - wind.value) * 0.08;
    }
    cogs.forEach((cog) => {
      cog.mesh.rotation.y = t * cog.speed;
    });
    planets.forEach((item) => {
      const a = t * item.body.speed + item.body.phase;
      const x = Math.cos(a) * item.body.orbit;
      const z = Math.sin(a) * item.body.orbit;
      item.group.position.set(x, 0.05, z);
      const mid = new THREE.Vector3(x * 0.5, 0.03, z * 0.5);
      item.arm.position.copy(mid);
      item.arm.lookAt(x, 0.05, z);
      item.arm.rotateX(Math.PI / 2);
      const len = Math.hypot(x, z);
      item.arm.scale.set(1, len, 1);
    });
    bearings.forEach((item) => {
      const a = t * 0.14 + item.phase;
      item.bead.position.set(Math.cos(a) * item.orbit, 0.022, Math.sin(a) * item.orbit);
    });
  }

  function tick(now) {
    pose(now);
    renderer.render(scene, camera);
    if (!reduce) {
      frame = window.requestAnimationFrame(tick);
    }
  }

  layout();
  pose(0);
  renderer.render(scene, camera);
  if (!reduce) {
    frame = window.requestAnimationFrame(tick);
  }
  window.addEventListener("resize", () => {
    layout();
    if (reduce) {
      renderer.render(scene, camera);
    }
  });
}

const ether = document.querySelector("[data-ether]");
if (ether) {
  paintOrrery(ether);
}
