import { cp, mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const source = fileURLToPath(new URL("./dist/", import.meta.url));
const destination = fileURLToPath(new URL("../", import.meta.url));

await mkdir(path.join(destination, "assets"), { recursive: true });
await cp(path.join(source, "index.html"), path.join(destination, "index.html"));
await cp(path.join(source, "akasha-mark.svg"), path.join(destination, "akasha-mark.svg"));
await cp(path.join(source, "assets"), path.join(destination, "assets"), {
  recursive: true,
  force: true,
});

