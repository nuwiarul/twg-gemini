import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";
import { execSync } from "node:child_process";

const UNSUPPORTED_BEVY_EXTENSIONS = new Set([
  "KHR_draco_mesh_compression",
  "KHR_mesh_quantization",
  "EXT_meshopt_compression",
  "FB_ngon_encoding",
]);

function resolveRequireBase() {
  // 1. Try local package.json if installed in project/scripts
  const localPkg = path.resolve("scripts", "package.json");
  if (fs.existsSync(localPkg)) {
    const r = createRequire(localPkg);
    try {
      r.resolve("@gltf-transform/core");
      return r;
    } catch {
      // fall through
    }
  }

  // 2. Resolve from globally installed @gltf-transform/cli
  const globalRoot = execSync("npm root -g", { encoding: "utf8" }).trim();
  const cliPkg = path.join(globalRoot, "@gltf-transform", "cli", "package.json");
  if (fs.existsSync(cliPkg)) {
    return createRequire(cliPkg);
  }

  throw new Error(
    "Could not locate @gltf-transform/core. Install @gltf-transform/cli globally or in scripts/."
  );
}

export function sniffImageMime(bytes) {
  if (!bytes || bytes.length < 12) return null;
  if (
    bytes[0] === 0x89 &&
    bytes[1] === 0x50 &&
    bytes[2] === 0x4e &&
    bytes[3] === 0x47
  ) {
    return "image/png";
  }
  if (bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff) {
    return "image/jpeg";
  }
  if (
    bytes[0] === 0x52 &&
    bytes[1] === 0x49 &&
    bytes[2] === 0x46 &&
    bytes[3] === 0x46 &&
    bytes[8] === 0x57 &&
    bytes[9] === 0x45 &&
    bytes[10] === 0x42 &&
    bytes[11] === 0x50
  ) {
    return "image/webp";
  }
  return null;
}

export function sha256Hex(buffer) {
  return crypto.createHash("sha256").update(buffer).digest("hex");
}

function fixTextureMimes(doc) {
  for (const tex of doc.getRoot().listTextures()) {
    const img = tex.getImage();
    const actual = sniffImageMime(img);
    if (actual && tex.getMimeType() !== actual) {
      tex.setMimeType(actual);
    }
  }
}

function stripUnsupportedExtensions(doc) {
  for (const ext of doc.getRoot().listExtensionsUsed()) {
    if (UNSUPPORTED_BEVY_EXTENSIONS.has(ext.extensionName)) {
      ext.dispose();
    }
  }
}

function stripUnusedTexCoords(doc) {
  const used = new Set([0]);
  for (const mat of doc.getRoot().listMaterials()) {
    for (const info of [
      mat.getBaseColorTextureInfo(),
      mat.getMetallicRoughnessTextureInfo(),
      mat.getNormalTextureInfo(),
      mat.getOcclusionTextureInfo(),
      mat.getEmissiveTextureInfo(),
    ]) {
      if (info) used.add(info.getTexCoord());
    }
  }
  for (const mesh of doc.getRoot().listMeshes()) {
    for (const prim of mesh.listPrimitives()) {
      for (const sem of prim.listSemantics()) {
        const match = /^TEXCOORD_(\d+)$/.exec(sem);
        if (match && !used.has(Number(match[1]))) {
          prim.setAttribute(sem, null);
        }
      }
    }
  }
}

function computeSmoothNormals(doc) {
  const buf = doc.getRoot().listBuffers()[0] || doc.createBuffer();
  for (const mesh of doc.getRoot().listMeshes()) {
    for (const prim of mesh.listPrimitives()) {
      if (prim.getMode() !== 4) continue; // TRIANGLES
      const pos = prim.getAttribute("POSITION");
      if (!pos) continue;
      const vCount = pos.getCount();
      if (vCount === 0) continue;

      const nx = new Float32Array(vCount);
      const ny = new Float32Array(vCount);
      const nz = new Float32Array(vCount);
      const indices = prim.getIndices();
      const triCount = indices
        ? Math.floor(indices.getCount() / 3)
        : Math.floor(vCount / 3);
      const p0 = [0, 0, 0];
      const p1 = [0, 0, 0];
      const p2 = [0, 0, 0];

      for (let t = 0; t < triCount; t++) {
        const i0 = indices ? indices.getScalar(t * 3) : t * 3;
        const i1 = indices ? indices.getScalar(t * 3 + 1) : t * 3 + 1;
        const i2 = indices ? indices.getScalar(t * 3 + 2) : t * 3 + 2;
        if (i0 >= vCount || i1 >= vCount || i2 >= vCount) continue;

        pos.getElement(i0, p0);
        pos.getElement(i1, p1);
        pos.getElement(i2, p2);
        const e1x = p1[0] - p0[0];
        const e1y = p1[1] - p0[1];
        const e1z = p1[2] - p0[2];
        const e2x = p2[0] - p0[0];
        const e2y = p2[1] - p0[1];
        const e2z = p2[2] - p0[2];
        const cx = e1y * e2z - e1z * e2y;
        const cy = e1z * e2x - e1x * e2z;
        const cz = e1x * e2y - e1y * e2x;

        nx[i0] += cx;
        ny[i0] += cy;
        nz[i0] += cz;
        nx[i1] += cx;
        ny[i1] += cy;
        nz[i1] += cz;
        nx[i2] += cx;
        ny[i2] += cy;
        nz[i2] += cz;
      }

      const out = new Float32Array(vCount * 3);
      for (let i = 0; i < vCount; i++) {
        const x = nx[i];
        const y = ny[i];
        const z = nz[i];
        const len = Math.hypot(x, y, z);
        if (len > 1e-8) {
          out[i * 3] = x / len;
          out[i * 3 + 1] = y / len;
          out[i * 3 + 2] = z / len;
        } else {
          out[i * 3 + 1] = 1.0;
        }
      }

      const acc = doc
        .createAccessor()
        .setType("VEC3")
        .setArray(out)
        .setBuffer(buf);
      prim.setAttribute("NORMAL", acc);
    }
  }
}

export function getDocStats(doc) {
  let tris = 0;
  let verts = 0;
  for (const mesh of doc.getRoot().listMeshes()) {
    for (const prim of mesh.listPrimitives()) {
      const idx = prim.getIndices();
      const pos = prim.getAttribute("POSITION");
      if (idx) tris += idx.getCount() / 3;
      else if (pos) tris += pos.getCount() / 3;
      if (pos) verts += pos.getCount();
    }
  }
  return {
    tris: Math.round(tris),
    verts,
    animations: doc.getRoot().listAnimations().length,
    skins: doc.getRoot().listSkins().length,
    textures: doc.getRoot().listTextures().length,
  };
}

export function verifyGlbBuffer(buf, label = "glb") {
  const b = Buffer.from(buf);
  if (b.length < 20 || b.subarray(0, 4).toString("ascii") !== "glTF") {
    throw new Error(`${label}: not a valid GLB header`);
  }
  const jsonLen = b.readUInt32LE(12);
  const json = JSON.parse(b.subarray(20, 20 + jsonLen).toString("utf8"));
  const used = new Set([
    ...(json.extensionsUsed || []),
    ...(json.extensionsRequired || []),
  ]);
  for (const bad of UNSUPPORTED_BEVY_EXTENSIONS) {
    if (used.has(bad)) {
      throw new Error(`${label}: unsupported Bevy extension remaining: ${bad}`);
    }
  }
  const rest = 20 + jsonLen;
  if (rest + 8 <= b.length && b.readUInt32LE(rest + 4) === 0x004e4942) {
    const binLen = b.readUInt32LE(rest);
    const bin = b.subarray(rest + 8, rest + 8 + binLen);
    for (const img of json.images || []) {
      if (img.bufferView == null) continue;
      const bv = json.bufferViews?.[img.bufferView];
      if (!bv) continue;
      const off = bv.byteOffset || 0;
      const actual = sniffImageMime(bin.subarray(off, off + 12));
      if (actual && img.mimeType !== actual) {
        throw new Error(
          `${label}: MIME mismatch declared=${img.mimeType} actual=${actual}`
        );
      }
    }
  }
  return json;
}

export async function initGltfPipeline() {
  const req = resolveRequireBase();
  const imp = (spec) => import(pathToFileURL(req.resolve(spec)).href);

  const { NodeIO, Logger } = await imp("@gltf-transform/core");
  const { ALL_EXTENSIONS } = await imp("@gltf-transform/extensions");
  const { dedup, prune, weld, simplify, resample, join, flatten } = await imp(
    "@gltf-transform/functions"
  );
  const { MeshoptSimplifier } = await imp("meshoptimizer");
  const draco3d = req("draco3dgltf");
  const sharp = req("sharp");

  await MeshoptSimplifier.ready;

  const io = new NodeIO()
    .registerExtensions(ALL_EXTENSIONS)
    .registerDependencies({
      "draco3d.decoder": await draco3d.createDecoderModule(),
      "draco3d.encoder": await draco3d.createEncoderModule(),
    })
    .setLogger(new Logger(Logger.Verbosity.SILENT));

  async function normalizeLod0Textures(doc) {
    // Ensure Bevy-standard PNG/JPEG textures (convert any WebP embedded textures to PNG/JPEG)
    for (const tex of doc.getRoot().listTextures()) {
      const img = tex.getImage();
      if (!img) continue;
      const actual = sniffImageMime(img);
      if (actual === "image/webp") {
        const s = sharp(Buffer.from(img));
        const meta = await s.metadata();
        if (meta.hasAlpha) {
          const out = await s.png({ compressionLevel: 9 }).toBuffer();
          tex.setImage(new Uint8Array(out)).setMimeType("image/png");
        } else {
          const out = await s.jpeg({ quality: 90 }).toBuffer();
          tex.setImage(new Uint8Array(out)).setMimeType("image/jpeg");
        }
      } else if (actual && tex.getMimeType() !== actual) {
        tex.setMimeType(actual);
      }
    }
    // Dispose EXT_texture_webp if no webp textures remain
    for (const ext of doc.getRoot().listExtensionsUsed()) {
      if (ext.extensionName === "EXT_texture_webp") {
        ext.dispose();
      }
    }
  }

  async function compressTexturesForLod2(doc, maxDim = 256) {
    for (const tex of doc.getRoot().listTextures()) {
      const img = tex.getImage();
      if (!img) continue;
      try {
        const s = sharp(Buffer.from(img));
        const meta = await s.metadata();
        let hasTransparency = false;
        if (meta.hasAlpha) {
          const st = await s.clone().stats();
          if (st.channels.length >= 4 && st.channels[3].min < 250) {
            hasTransparency = true;
          }
        }
        const pipeline = s.clone().resize({
          width: maxDim,
          height: maxDim,
          fit: "inside",
          withoutEnlargement: true,
        });
        if (hasTransparency) {
          const out = await pipeline
            .png({ compressionLevel: 9, palette: true })
            .toBuffer();
          tex.setImage(new Uint8Array(out)).setMimeType("image/png");
        } else {
          const out = await pipeline
            .jpeg({ quality: 82, mozjpeg: true })
            .toBuffer();
          tex.setImage(new Uint8Array(out)).setMimeType("image/jpeg");
        }
      } catch {
        // Keep original texture if sharp cannot decode it
      }
    }
    for (const ext of doc.getRoot().listExtensionsUsed()) {
      if (ext.extensionName === "EXT_texture_webp") {
        ext.dispose();
      }
    }
  }

  async function convertModelFile({
    srcPath,
    lod0OutPath,
    lod2OutPath,
    expectAnimMode = "any",
  }) {
    const doc = await io.read(srcPath);
    doc.setLogger(new Logger(Logger.Verbosity.SILENT));
    stripUnsupportedExtensions(doc);
    fixTextureMimes(doc);
    stripUnusedTexCoords(doc);

    const initialAnimCount = doc.getRoot().listAnimations().length;
    if (expectAnimMode === "none" && initialAnimCount > 0) {
      throw new Error(
        `Model idle tidak boleh memiliki animasi, tetapi ditemukan ${initialAnimCount} animasi di ${path.basename(srcPath)}`
      );
    }
    if (expectAnimMode === "required" && initialAnimCount === 0) {
      throw new Error(
        `Model walk wajib memiliki animasi jalan, tetapi ditemukan 0 animasi di ${path.basename(srcPath)}`
      );
    }

    const isSkinned = doc.getRoot().listSkins().length > 0;
    const hasAnim = initialAnimCount > 0;

    // --- LOD0: Bevy-ready full quality (Draco decoded, deduped, welded, resampled, MIME fixed) ---
    await doc.transform(
      dedup(),
      resample({ tolerance: 0.001 }),
      weld(),
      prune()
    );
    await normalizeLod0Textures(doc);
    stripUnsupportedExtensions(doc);
    fixTextureMimes(doc);

    const lod0Buf = await io.writeBinary(doc);
    verifyGlbBuffer(lod0Buf, path.basename(lod0OutPath));
    const lod0Stats = getDocStats(doc);

    fs.mkdirSync(path.dirname(lod0OutPath), { recursive: true });
    fs.writeFileSync(lod0OutPath, lod0Buf);

    // --- LOD2: Lightweight simplified mesh + smooth indexed normals + downscaled textures ---
    let lod2Buf;
    let lod2Stats;
    try {
      const doc2 = await io.readBinary(lod0Buf);
      doc2.setLogger(new Logger(Logger.Verbosity.SILENT));
      stripUnsupportedExtensions(doc2);
      fixTextureMimes(doc2);

      // Remove faceted normals/tangents on triangle meshes so weld() merges vertices across hard faces
      let hadNormals = false;
      for (const mesh of doc2.getRoot().listMeshes()) {
        for (const prim of mesh.listPrimitives()) {
          if (prim.getMode() === 4) {
            if (prim.getAttribute("NORMAL")) {
              hadNormals = true;
              prim.setAttribute("NORMAL", null);
            }
            if (prim.getAttribute("TANGENT")) {
              prim.setAttribute("TANGENT", null);
            }
          }
        }
      }

      const t = lod0Stats.tris;
      const ops = [weld()];
      if (t > 400) {
        const ratio =
          t > 12000 ? 0.22 : t > 5000 ? 0.28 : t > 1500 ? 0.38 : 0.55;
        const error =
          t > 12000 ? 0.05 : t > 5000 ? 0.04 : t > 1500 ? 0.03 : 0.02;
        ops.push(
          simplify({
            simplifier: MeshoptSimplifier,
            ratio,
            error,
            lockBorder: false,
          })
        );
      }
      ops.push(dedup(), resample({ tolerance: 0.005 }));
      if (!isSkinned && !hasAnim) {
        ops.push(flatten(), join());
      }
      ops.push(prune());

      await doc2.transform(...ops);
      if (hadNormals) {
        computeSmoothNormals(doc2);
      }
      await compressTexturesForLod2(doc2, 256);
      await doc2.transform(prune());
      stripUnsupportedExtensions(doc2);
      fixTextureMimes(doc2);

      const candidateStats = getDocStats(doc2);
      if (candidateStats.tris === 0 && lod0Stats.tris > 0) {
        throw new Error("LOD2 simplified to 0 triangles, falling back");
      }

      lod2Buf = await io.writeBinary(doc2);
      verifyGlbBuffer(lod2Buf, path.basename(lod2OutPath));
      lod2Stats = candidateStats;
    } catch {
      // Safe fallback: texture downscale + prune only (preserves geometry 100%)
      const docFb = await io.readBinary(lod0Buf);
      docFb.setLogger(new Logger(Logger.Verbosity.SILENT));
      await compressTexturesForLod2(docFb, 256);
      await docFb.transform(dedup(), prune());
      stripUnsupportedExtensions(docFb);
      fixTextureMimes(docFb);
      lod2Buf = await io.writeBinary(docFb);
      verifyGlbBuffer(lod2Buf, path.basename(lod2OutPath));
      lod2Stats = getDocStats(docFb);
    }

    fs.mkdirSync(path.dirname(lod2OutPath), { recursive: true });
    fs.writeFileSync(lod2OutPath, lod2Buf);

    return {
      lod0: {
        sha256: sha256Hex(lod0Buf),
        size_bytes: lod0Buf.byteLength,
        tris: lod0Stats.tris,
        verts: lod0Stats.verts,
        animations: lod0Stats.animations,
      },
      lod2: {
        sha256: sha256Hex(lod2Buf),
        size_bytes: lod2Buf.byteLength,
        tris: lod2Stats.tris,
        verts: lod2Stats.verts,
        animations: lod2Stats.animations,
      },
    };
  }

  return { convertModelFile };
}
