#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";
import { initGltfPipeline } from "./gltf-pipeline.mjs";

const SOURCE_ROOT =
  process.env.TWG_LEGACY_PUBLIC || "E:\\tactical_game\\3dmap\\public";
const SOURCE_MODELS_TS =
  process.env.TWG_LEGACY_MODELS_TS ||
  "E:\\tactical_game\\3dmap\\src\\utils\\models.ts";
const TARGET_ASSETS_DIR = path.resolve(process.env.ASSETS_DIR || "assets");
const TARGET_MODELS_DIR = path.join(TARGET_ASSETS_DIR, "models");
const TARGET_CATALOG_PATH = path.join(
  TARGET_ASSETS_DIR,
  "catalog",
  "models.json"
);

const ANIMATED_CATEGORIES = new Set(["peoples", "stackholders", "units"]);

const GROUP_TO_CATEGORY = [
  ["BANGUNANS", "bangunans"],
  ["KENDARAAN_UMUMS", "kendaraan_umums"],
  ["KENDARAANS", "kendaraans"],
  ["PEOPLES", "peoples"],
  ["STACKHOLDERS", "stackholders"],
  ["TOOLS", "tools"],
  ["UNITS", "units"],
];

function parseLegacyModelsTs(tsPath) {
  let code = fs.readFileSync(tsPath, "utf8");
  code = code.replace(/^import\s+type\s+.*?;\s*$/gm, "");
  code = code.replace(/export\s+const\s+(\w+)\s*:\s*[^=]+=/g, "const $1 =");
  const script = `(() => {\n${code}\nreturn { UNITS, STACKHOLDERS, PEOPLES, KENDARAANS, BANGUNANS, TOOLS, KENDARAAN_UMUMS };\n})()`;
  return vm.runInNewContext(script, {});
}

function normalizeRel(urlPath) {
  return String(urlPath || "")
    .replace(/^\/+/, "")
    .replace(/\\/g, "/");
}

function isDracoGlb(filePath) {
  if (!fs.existsSync(filePath)) return false;
  try {
    const buf = fs.readFileSync(filePath);
    if (buf.length < 20 || buf.subarray(0, 4).toString("ascii") !== "glTF") {
      return false;
    }
    const jsonLen = buf.readUInt32LE(12);
    const json = JSON.parse(buf.subarray(20, 20 + jsonLen).toString("utf8"));
    const used = [
      ...(json.extensionsRequired || []),
      ...(json.extensionsUsed || []),
    ];
    return used.includes("KHR_draco_mesh_compression");
  } catch {
    return false;
  }
}

function resolveBestSource(relUrl) {
  const rel = normalizeRel(relUrl);
  const candidates = [rel];
  if (rel.includes("_draco_")) {
    candidates.push(rel.replace("_draco_", "_anim_"));
    candidates.push(rel.replace("_draco_", "_"));
    candidates.push(rel.replace("_draco_1.glb", "_1.glb"));
    candidates.push(rel.replace("_draco_1.glb", ".glb"));
  }
  if (rel.endsWith("_draco.glb")) {
    candidates.push(rel.replace("_draco.glb", ".glb"));
  }
  const uniq = [...new Set(candidates.filter(Boolean))];
  for (const c of uniq) {
    const abs = path.join(SOURCE_ROOT, c);
    if (fs.existsSync(abs) && !isDracoGlb(abs)) return c;
  }
  for (const c of uniq) {
    const abs = path.join(SOURCE_ROOT, c);
    if (fs.existsSync(abs)) return c;
  }
  return null;
}

function cleanTargetFilename(rel, suffix = "") {
  const ext = path.extname(rel);
  let base = path.basename(rel, ext);
  base = base.replace(/_draco(_\d+)?$/i, "$1");
  return `${base}${suffix}.glb`;
}

function withLod2Suffix(relPath) {
  const parsed = path.posix.parse(relPath);
  return `${parsed.dir}/${parsed.name}_lod2.glb`;
}

function buildSlug(category, primaryFileRel) {
  const base = path.posix
    .basename(primaryFileRel, ".glb")
    .replace(/_idle$/i, "")
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, "_");
  return `${category}/${base}`;
}

async function main() {
  const startedAt = Date.now();
  if (!fs.existsSync(SOURCE_MODELS_TS)) {
    console.error(`Legacy models.ts not found at: ${SOURCE_MODELS_TS}`);
    process.exit(1);
  }

  console.log("Initializing glTF-Transform + Draco + MeshoptSimplifier + Sharp pipeline...");
  const pipeline = await initGltfPipeline();

  // Clean existing generated models dir so stale starter files don't linger
  fs.mkdirSync(TARGET_MODELS_DIR, { recursive: true });
  for (const [, cat] of GROUP_TO_CATEGORY) {
    const catDir = path.join(TARGET_MODELS_DIR, cat);
    if (fs.existsSync(catDir)) {
      fs.rmSync(catDir, { recursive: true, force: true });
    }
    fs.mkdirSync(catDir, { recursive: true });
  }
  fs.mkdirSync(path.dirname(TARGET_CATALOG_PATH), { recursive: true });

  const groups = parseLegacyModelsTs(SOURCE_MODELS_TS);
  const catalog = [];
  const usedSlugs = new Set();

  let totalModels = 0;
  let animatedModels = 0;
  let staticModels = 0;
  let totalLod0Bytes = 0;
  let totalLod2Bytes = 0;
  let totalLod0Tris = 0;
  let totalLod2Tris = 0;
  let errors = 0;

  for (const [groupKey, category] of GROUP_TO_CATEGORY) {
    const items = groups[groupKey] || [];
    console.log(`\n=== Converting category: ${category} (${items.length} models) ===`);

    for (let idx = 0; idx < items.length; idx++) {
      const item = items[idx];
      const origModelRel = normalizeRel(item.modelUrl);
      const origWalkRel = item.walk?.modelUrl
        ? normalizeRel(item.walk.modelUrl)
        : null;

      const isAnimatedPair =
        ANIMATED_CATEGORIES.has(category) &&
        Boolean(origWalkRel) &&
        origWalkRel !== origModelRel;

      const bestPrimarySrc = resolveBestSource(origModelRel);
      if (!bestPrimarySrc) {
        console.error(`  [MISSING] ${item.name} (${origModelRel})`);
        errors++;
        continue;
      }

      const primaryFilename = cleanTargetFilename(bestPrimarySrc);
      const primaryRelPath = `models/${category}/${primaryFilename}`;
      const primaryLod2RelPath = withLod2Suffix(primaryRelPath);

      let slug = buildSlug(category, primaryFilename);
      let n = 2;
      while (usedSlugs.has(slug)) {
        slug = `${buildSlug(category, primaryFilename)}_${n++}`;
      }
      usedSlugs.add(slug);

      try {
        const primaryRes = await pipeline.convertModelFile({
          srcPath: path.join(SOURCE_ROOT, bestPrimarySrc),
          lod0OutPath: path.join(TARGET_ASSETS_DIR, primaryRelPath),
          lod2OutPath: path.join(TARGET_ASSETS_DIR, primaryLod2RelPath),
          expectAnimMode: isAnimatedPair ? "none" : "any",
        });

        totalLod0Bytes += primaryRes.lod0.size_bytes;
        totalLod2Bytes += primaryRes.lod2.size_bytes;
        totalLod0Tris += primaryRes.lod0.tris;
        totalLod2Tris += primaryRes.lod2.tris;

        let walkRelPath = null;
        let walkLod2RelPath = null;
        let walkRes = null;

        if (isAnimatedPair) {
          const bestWalkSrc = resolveBestSource(origWalkRel);
          if (!bestWalkSrc) {
            throw new Error(`Missing walk source file: ${origWalkRel}`);
          }
          const walkFilename = cleanTargetFilename(bestWalkSrc);
          walkRelPath = `models/${category}/${walkFilename}`;
          walkLod2RelPath = withLod2Suffix(walkRelPath);

          walkRes = await pipeline.convertModelFile({
            srcPath: path.join(SOURCE_ROOT, bestWalkSrc),
            lod0OutPath: path.join(TARGET_ASSETS_DIR, walkRelPath),
            lod2OutPath: path.join(TARGET_ASSETS_DIR, walkLod2RelPath),
            expectAnimMode: "required",
          });

          totalLod0Bytes += walkRes.lod0.size_bytes;
          totalLod2Bytes += walkRes.lod2.size_bytes;
          totalLod0Tris += walkRes.lod0.tris;
          totalLod2Tris += walkRes.lod2.tris;
          animatedModels++;
        } else {
          staticModels++;
        }

        catalog.push({
          id: item.id,
          slug,
          name: item.name,
          category,
          model_type: isAnimatedPair ? "animated" : "static",
          model_path: primaryRelPath,
          model_sha256: primaryRes.lod0.sha256,
          model_size_bytes: primaryRes.lod0.size_bytes,
          model_tris: primaryRes.lod0.tris,
          lod2_path: primaryLod2RelPath,
          lod2_sha256: primaryRes.lod2.sha256,
          lod2_size_bytes: primaryRes.lod2.size_bytes,
          lod2_tris: primaryRes.lod2.tris,
          walk_model_path: walkRelPath,
          walk_sha256: walkRes ? walkRes.lod0.sha256 : null,
          walk_size_bytes: walkRes ? walkRes.lod0.size_bytes : 0,
          walk_tris: walkRes ? walkRes.lod0.tris : 0,
          walk_lod2_path: walkLod2RelPath,
          walk_lod2_sha256: walkRes ? walkRes.lod2.sha256 : null,
          walk_lod2_size_bytes: walkRes ? walkRes.lod2.size_bytes : 0,
          walk_lod2_tris: walkRes ? walkRes.lod2.tris : 0,
          default_height: Number(item.height || 10),
          default_rotation_deg: [
            Number(item.rotation?.x || 0),
            Number(item.rotation?.y || 0),
            Number(item.rotation?.z || 0),
          ],
          ready: true,
        });

        totalModels++;
        if ((idx + 1) % 20 === 0 || idx + 1 === items.length) {
          console.log(`  [${category}] ${idx + 1}/${items.length} converted`);
        }
      } catch (err) {
        console.error(`  [ERROR] ${category}/${item.name}: ${err.message}`);
        errors++;
      }
    }
  }

  fs.writeFileSync(TARGET_CATALOG_PATH, JSON.stringify(catalog, null, 2));

  const elapsedSec = ((Date.now() - startedAt) / 1000).toFixed(1);
  const lod0Mb = (totalLod0Bytes / 1024 / 1024).toFixed(1);
  const lod2Mb = (totalLod2Bytes / 1024 / 1024).toFixed(1);
  const byteSavings = Math.round((1 - totalLod2Bytes / Math.max(1, totalLod0Bytes)) * 100);
  const triSavings = Math.round((1 - totalLod2Tris / Math.max(1, totalLod0Tris)) * 100);

  console.log("\n==========================================");
  console.log(`Converted models : ${totalModels} (static=${staticModels}, animated=${animatedModels}, errors=${errors})`);
  console.log(`Total GLB files  : ${totalModels * 2 + animatedModels * 2} (LOD0 + LOD2)`);
  console.log(`LOD0 total size  : ${lod0Mb} MB (${totalLod0Tris.toLocaleString()} tris)`);
  console.log(`LOD2 total size  : ${lod2Mb} MB (${totalLod2Tris.toLocaleString()} tris)`);
  console.log(`LOD2 reduction   : -${byteSavings}% size, -${triSavings}% triangles`);
  console.log(`Catalog written  : ${TARGET_CATALOG_PATH}`);
  console.log(`Elapsed time     : ${elapsedSec}s`);
  console.log("==========================================");

  if (errors > 0) {
    process.exit(1);
  }
}

main();
