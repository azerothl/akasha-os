// Rasterize logo SVGs. From this directory:
//   npm install --no-save @resvg/resvg-js
//   node rasterize.mjs
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { Resvg } from "@resvg/resvg-js";

const here = dirname(fileURLToPath(import.meta.url));
const outDir = join(here, "export");
mkdirSync(outDir, { recursive: true });

function render(svgPath, size, dest) {
  const svg = readFileSync(svgPath);
  const resvg = new Resvg(svg, {
    fitTo: { mode: "width", value: size },
    background: "transparent",
  });
  writeFileSync(dest, resvg.render().asPng());
  console.log(`  ${size}px -> ${dest}`);
}

const logo = join(here, "logo.svg");
const small = join(here, "favicon.svg");

for (const size of [16, 24, 32, 48]) {
  render(small, size, join(outDir, `icon-${size}.png`));
}
for (const size of [64, 128, 180, 256, 512]) {
  render(logo, size, join(outDir, `icon-${size}.png`));
}
render(logo, 360, join(outDir, "og-mark.png"));
