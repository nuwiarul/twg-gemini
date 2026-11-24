#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";
import { execFileSync } from "node:child_process";

const args = new Set(process.argv.slice(2));
const COPY_ALL = args.has("--all");
const SKIP_DRACO_DECODE = args.has("--skip-draco");

const SOURCE_ROOT = process.env.TWG_LEGACY_PUBLIC || "E:\\tactical_game\\3dmap\\public";
const SOURCE_MODELS_TS = process.env.TWG_LEGACY_MODELS_TS || "E:\\tactical_game\\3dmap\\src\\utils\\models.ts";
const TARGET_ASSETS_DIR = path.resolve("assets");
const TARGET_MODELS_DIR = path.join(TARGET_ASSETS_DIR, "models");
const TARGET_CATALOG_PATH = path.join(TARGET_ASSETS_DIR, "catalog", "models.json");

const STARTER_MODEL_REL_PATHS = new Set([
  // Units
  "units/polisi_walk.glb",
  "units/polwan_walk.glb",
  "units/brimob_walk.glb",
  "units/lantas_walk.glb",
  "units/samapta_walk.glb",
  "units/densus_88_walk.glb",
  "units/k9_walk.glb",
  "units/polisi_kuda_walk.glb",
  // Kendaraans
  "kendaraans/mobil_patwal_anim_1.glb",
  "kendaraans/mobil_samapta_anim_1.glb",
  "kendaraans/mobil_gegana_anim_1.glb",
  "kendaraans/mobil_damkar_anim_1.glb",
  "kendaraans/mobil_dokkes_anim_1.glb",
  "kendaraans/mobil_tni_anim_1.glb",
  "kendaraans/helikopter_polisi_anim_1.glb",
  "kendaraans/motor_patwal_anim_1.glb",
  "kendaraans/rantis_brimob_anim_1.glb",
  "kendaraans/awc_brimob_anim_1.glb",
  "kendaraans/kapal_polisi_anim_1.glb",
  "kendaraans/jet_tempur.glb",
  // Bangunans
  "bangunans/kantor_polisi_1.glb",
  "bangunans/pos_polisi_1.glb",
  "bangunans/kantor_pemerintah_1.glb",
  "bangunans/rumah_sakit_1.glb",
  "bangunans/hotel_1.glb",
  "bangunans/gedung_1.glb",
  "bangunans/tenda.glb",
  "bangunans/tower_cell.glb",
  // Tools / Alat
  "tools/barrier_1.glb",
  "tools/cone_1.glb",
  "tools/kawat_berduri_1.glb",
  "tools/pos_security.glb",
  "tools/portal_security.glb",
  // Peoples & Stakeholders
  "peoples/masyarakat_walk.glb",
  "peoples/perusuh_walk.glb",
  "stackholders/dishub_anim_1.glb",
  "stackholders/damkar_anim_1.glb",
  "stackholders/sar_anim_1.glb",
  "stackholders/polpp_anim_1.glb",
]);

function readGlbChunks(filePath) {
  const buf = fs.readFileSync(filePath);
  if (buf.length < 20 || buf.subarray(0, 4).toString("ascii") !== "glTF") {
    throw new Error(`Not a valid GLB file: ${filePath}`);
  }
  const version = buf.readUInt32LE(4);
  const jsonLen = buf.readUInt32LE(12);
  const jsonType = buf.readUInt32LE(16);
  if (jsonType !== 0x4e4f534a) {
    throw new Error(`Unexpected first chunk in GLB: ${filePath}`);
  }
  const json = JSON.parse(buf.subarray(20, 20 + jsonLen).toString("utf8"));
  const restOffset = 20 + jsonLen;
  let binChunkData = null;
  if (restOffset + 8 <= buf.length) {
    const binLen = buf.readUInt32LE(restOffset);
    const binType = buf.readUInt32LE(restOffset + 4);
    if (binType === 0x004e4942) {
      binChunkData = buf.subarray(restOffset + 8, restOffset + 8 + binLen);
    }
  }
  return { version, json, binChunkData };
}

function isDracoGlb(filePath) {
  if (!fs.existsSync(filePath)) return false;
  try {
    const { json } = readGlbChunks(filePath);
    const req = json.extensionsRequired || [];
    const used = json.extensionsUsed || [];
    return req.includes("KHR_draco_mesh_compression") || used.includes("KHR_draco_mesh_compression");
  } catch {
    return false;
  }
}

function sniffImageMime(bytes) {
  if (bytes.length >= 4 && bytes[0] === 0x89 && bytes[1] === 0x50 && bytes[2] === 0x4e && bytes[3] === 0x47) {
    return "image/png";
  }
  if (bytes.length >= 3 && bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff) {
    return "image/jpeg";
  }
  if (
    bytes.length >= 12 &&
    bytes.subarray(0, 4).toString("ascii") === "RIFF" &&
    bytes.subarray(8, 12).toString("ascii") === "WEBP"
  ) {
    return "image/webp";
  }
  return null;
}

function fixGlbMimeTypes(filePath) {
  const { version, json, binChunkData } = readGlbChunks(filePath);
  if (!binChunkData || !Array.isArray(json.images) || !Array.isArray(json.bufferViews)) {
    return false;
  }

  let patched = false;
  for (const img of json.images) {
    if (img.bufferView == null) continue;
    const bv = json.bufferViews[img.bufferView];
    if (!bv) continue;
    const offset = bv.byteOffset || 0;
    const slice = binChunkData.subarray(offset, offset + 12);
    const actualMime = sniffImageMime(slice);
    if (actualMime && img.mimeType !== actualMime) {
      img.mimeType = actualMime;
      patched = true;
    }
  }

  if (!patched) return false;

  const rawJsonBuf = Buffer.from(JSON.stringify(json), "utf8");
  const jsonPad = (4 - (rawJsonBuf.length % 4)) % 4;
  const paddedJsonBuf = jsonPad > 0 ? Buffer.concat([rawJsonBuf, Buffer.alloc(jsonPad, 0x20)]) : rawJsonBuf;

  const binPad = (4 - (binChunkData.length % 4)) % 4;
  const paddedBinBuf = binPad > 0 ? Buffer.concat([binChunkData, Buffer.alloc(binPad, 0x00)]) : binChunkData;

  const totalLen = 12 + 8 + paddedJsonBuf.length + 8 + paddedBinBuf.length;
  const header = Buffer.alloc(12);
  header.write("glTF", 0, "ascii");
  header.writeUInt32LE(version, 4);
  header.writeUInt32LE(totalLen, 8);

  const jsonHeader = Buffer.alloc(8);
  jsonHeader.writeUInt32LE(paddedJsonBuf.length, 0);
  jsonHeader.writeUInt32LE(0x4e4f534a, 4); // JSON

  const binHeader = Buffer.alloc(8);
  binHeader.writeUInt32LE(paddedBinBuf.length, 0);
  binHeader.writeUInt32LE(0x004e4942, 4); // BIN

  const out = Buffer.concat([header, jsonHeader, paddedJsonBuf, binHeader, paddedBinBuf]);
  fs.writeFileSync(filePath, out);
  return true;
}

function normalizeRel(urlPath) {
  return String(urlPath || "").replace(/^\/+/, "").replace(/\\/g, "/");
}

function resolveCandidatePaths(relUrl, walkRelUrl) {
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
  if (walkRelUrl) {
    candidates.push(normalizeRel(walkRelUrl));
  }
  if (rel.endsWith("_idle.glb")) {
    candidates.push(rel.replace("_idle.glb", "_walk.glb"));
  }

  return [...new Set(candidates.filter(Boolean))];
}

function findBestSourceRel(relUrl, walkRelUrl) {
  const candidates = resolveCandidatePaths(relUrl, walkRelUrl);
  // First pass: prefer uncompressed existing GLB
  for (const c of candidates) {
    const abs = path.join(SOURCE_ROOT, c);
    if (fs.existsSync(abs) && !isDracoGlb(abs)) {
      return { rel: c, isDraco: false };
    }
  }
  // Second pass: fall back to existing Draco GLB (will be decoded via gltf-transform)
  for (const c of candidates) {
    const abs = path.join(SOURCE_ROOT, c);
    if (fs.existsSync(abs)) {
      return { rel: c, isDraco: true };
    }
  }
  return null;
}

function parseLegacyModelsTs(tsPath) {
  let code = fs.readFileSync(tsPath, "utf8");
  code = code.replace(/^import\s+type\s+.*?;\s*$/gm, "");
  code = code.replace(/export\s+const\s+(\w+)\s*:\s*[^=]+=/g, "const $1 =");
  const script = `(() => {\n${code}\nreturn { UNITS, STACKHOLDERS, PEOPLES, KENDARAANS, BANGUNANS, TOOLS, KENDARAAN_UMUMS };\n})()`;
  return vm.runInNewContext(script, {});
}

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

function migrateSingleGlb(srcRel, dstRel) {
  const src = path.join(SOURCE_ROOT, srcRel);
  const dst = path.join(TARGET_MODELS_DIR, dstRel);
  if (!fs.existsSync(src)) return { ok: false, decoded: false, mimeFixed: false };
  ensureDir(path.dirname(dst));

  let decoded = false;
  if (isDracoGlb(src)) {
    if (SKIP_DRACO_DECODE) {
      return { ok: false, decoded: false, mimeFixed: false };
    }
    try {
      const npxBin = process.platform === "win32" ? "npx.cmd" : "npx";
      execFileSync(npxBin, ["-y", "@gltf-transform/cli", "copy", src, dst], {
        stdio: "pipe",
      });
      decoded = true;
    } catch (err) {
      console.warn(`Failed to decode Draco model ${srcRel}: ${err.message}`);
      return { ok: false, decoded: false, mimeFixed: false };
    }
  } else {
    fs.copyFileSync(src, dst);
  }

  const mimeFixed = fixGlbMimeTypes(dst);
  return { ok: fs.existsSync(dst) && !isDracoGlb(dst), decoded, mimeFixed };
}

function verifyMigratedModels() {
  let total = 0;
  let dracoCount = 0;
  let mimeMismatch = 0;

  function walk(dir) {
    if (!fs.existsSync(dir)) return;
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(full);
      } else if (full.endsWith(".glb")) {
        total++;
        const { json, binChunkData } = readGlbChunks(full);
        const req = json.extensionsRequired || [];
        const used = json.extensionsUsed || [];
        if (req.includes("KHR_draco_mesh_compression") || used.includes("KHR_draco_mesh_compression")) {
          dracoCount++;
          console.error(`DRACO REMAINING: ${full}`);
        }
        if (binChunkData && Array.isArray(json.images)) {
          for (const img of json.images) {
            if (img.bufferView == null) continue;
            const bv = json.bufferViews[img.bufferView];
            const actual = sniffImageMime(binChunkData.subarray(bv.byteOffset || 0, (bv.byteOffset || 0) + 12));
            if (actual && img.mimeType !== actual) {
              mimeMismatch++;
              console.error(`MIME MISMATCH: ${full} declared=${img.mimeType} actual=${actual}`);
            }
          }
        }
      }
    }
  }

  walk(TARGET_MODELS_DIR);
  return { total, dracoCount, mimeMismatch };
}

function main() {
  if (!fs.existsSync(SOURCE_MODELS_TS)) {
    console.error(`Legacy models.ts not found at: ${SOURCE_MODELS_TS}`);
    process.exit(1);
  }

  ensureDir(TARGET_MODELS_DIR);
  ensureDir(path.dirname(TARGET_CATALOG_PATH));

  const groups = parseLegacyModelsTs(SOURCE_MODELS_TS);
  const allEntries = [
    ...groups.UNITS,
    ...groups.KENDARAANS,
    ...groups.BANGUNANS,
    ...groups.TOOLS,
    ...groups.PEOPLES,
    ...groups.STACKHOLDERS,
    ...groups.KENDARAAN_UMUMS,
  ];

  let copiedCount = 0;
  let decodedCount = 0;
  let mimeFixedCount = 0;
  const processedSet = new Set();
  const catalog = [];

  for (const item of allEntries) {
    const origRel = normalizeRel(item.modelUrl);
    const walkRelOrig = item.walk?.modelUrl ? normalizeRel(item.walk.modelUrl) : null;

    const best = findBestSourceRel(origRel, walkRelOrig);
    const bestWalk = walkRelOrig ? findBestSourceRel(walkRelOrig, null) : null;

    // Prefer starter path when explicitly in starter list (e.g. units/brimob_walk.glb)
    let chosenRel = best ? best.rel : origRel;
    if (walkRelOrig && STARTER_MODEL_REL_PATHS.has(walkRelOrig) && fs.existsSync(path.join(SOURCE_ROOT, walkRelOrig))) {
      chosenRel = walkRelOrig;
    }

    const shouldMigrate = COPY_ALL || STARTER_MODEL_REL_PATHS.has(chosenRel);
    if (shouldMigrate && !processedSet.has(chosenRel)) {
      processedSet.add(chosenRel);
      const res = migrateSingleGlb(chosenRel, chosenRel);
      if (res.ok) {
        copiedCount++;
        if (res.decoded) decodedCount++;
        if (res.mimeFixed) mimeFixedCount++;
      }
    }

    if (bestWalk && (COPY_ALL || STARTER_MODEL_REL_PATHS.has(bestWalk.rel)) && !processedSet.has(bestWalk.rel)) {
      processedSet.add(bestWalk.rel);
      const res = migrateSingleGlb(bestWalk.rel, bestWalk.rel);
      if (res.ok) {
        copiedCount++;
        if (res.decoded) decodedCount++;
        if (res.mimeFixed) mimeFixedCount++;
      }
    }

    const dstPath = path.join(TARGET_MODELS_DIR, chosenRel);
    const targetReady = fs.existsSync(dstPath) && !isDracoGlb(dstPath);

    catalog.push({
      id: item.id,
      name: item.name,
      category: item.kategori || "other",
      model_path: `models/${chosenRel}`,
      walk_model_path: bestWalk ? `models/${bestWalk.rel}` : null,
      default_height: Number(item.height || 10),
      default_rotation_deg: [
        Number(item.rotation?.x || 0),
        Number(item.rotation?.y || 0),
        Number(item.rotation?.z || 0),
      ],
      ready: targetReady,
    });
  }

  fs.writeFileSync(TARGET_CATALOG_PATH, JSON.stringify(catalog, null, 2));
  const readyCount = catalog.filter((c) => c.ready).length;
  const verified = verifyMigratedModels();

  console.log(`Catalog written: ${TARGET_CATALOG_PATH} (${catalog.length} entries, ${readyCount} ready in assets/models)`);
  console.log(`Migrated GLBs: ${copiedCount} (Draco decoded: ${decodedCount}, MIME patched: ${mimeFixedCount})`);
  console.log(`Verification: total=${verified.total}, dracoRemaining=${verified.dracoCount}, mimeMismatches=${verified.mimeMismatch}`);

  if (verified.dracoCount > 0 || verified.mimeMismatch > 0) {
    process.exit(1);
  }
}

main();
