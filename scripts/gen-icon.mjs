// Rasterize the SVG app icon to a 1024x1024 PNG for `tauri icon`.
import sharp from "sharp";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const src = join(root, "src-tauri", "icon-source.svg");
const out = join(root, "src-tauri", "icon-1024.png");

await sharp(src, { density: 384 }).resize(1024, 1024).png().toFile(out);
console.log("Skrev", out);
