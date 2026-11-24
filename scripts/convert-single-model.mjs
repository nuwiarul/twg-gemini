#!/usr/bin/env node
import { initGltfPipeline } from "./gltf-pipeline.mjs";

function parseArgs(argv) {
  const out = {};
  for (let i = 0; i < argv.length; i++) {
    const k = argv[i];
    if (k.startsWith("--")) {
      out[k.slice(2)] = argv[i + 1];
      i++;
    }
  }
  return out;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const srcPath = args.input;
  const lod0OutPath = args.lod0;
  const lod2OutPath = args.lod2;
  const expectAnimMode = args.anim || "any";

  if (!srcPath || !lod0OutPath || !lod2OutPath) {
    process.stdout.write(
      JSON.stringify({
        ok: false,
        error: "Usage: convert-single-model.mjs --input <src> --lod0 <dst0> --lod2 <dst2> [--anim any|none|required]",
      }) + "\n"
    );
    process.exit(1);
  }

  try {
    const pipeline = await initGltfPipeline();
    const res = await pipeline.convertModelFile({
      srcPath,
      lod0OutPath,
      lod2OutPath,
      expectAnimMode,
    });
    process.stdout.write(JSON.stringify({ ok: true, ...res }) + "\n");
  } catch (err) {
    process.stdout.write(
      JSON.stringify({ ok: false, error: err?.message || String(err) }) + "\n"
    );
    process.exit(1);
  }
}

main();
