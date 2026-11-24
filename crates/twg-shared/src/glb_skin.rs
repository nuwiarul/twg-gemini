use std::collections::HashSet;

use serde_json::Value;

/// Maximum tolerated deviation from unit sum (`1.0`) for glTF `WEIGHTS_0`.
pub const WEIGHT_SUM_EPSILON: f32 = 1e-6;

const GLB_MAGIC: &[u8; 4] = b"glTF";
const GLB_CHUNK_JSON: u32 = 0x4E4F_534A; // "JSON"
const GLB_CHUNK_BIN: u32 = 0x004E_4942; // "BIN\0"

const COMPONENT_UNSIGNED_BYTE: u64 = 5121;
const COMPONENT_UNSIGNED_SHORT: u64 = 5123;
const COMPONENT_FLOAT: u64 = 5126;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GlbSkinNormalizeReport {
    pub repaired_vertices: usize,
    pub repaired_accessors: usize,
}

/// Returns `true` if a 4-bone skinning weight vector contains invalid entries (NaN, Inf, `< 0`)
/// or its sum deviates from `1.0` beyond `WEIGHT_SUM_EPSILON`.
#[inline]
pub fn joint_weight_needs_normalization(w: [f32; 4]) -> bool {
    w.iter().any(|&c| !c.is_finite() || c < 0.0)
        || ((w[0] + w[1] + w[2] + w[3]) - 1.0).abs() > WEIGHT_SUM_EPSILON
}

/// Sanitizes and normalizes a single glTF `WEIGHTS_0` vector so its 4 bone weights sum to exact
/// `1.0_f32`.
///
/// Unnormalized weights (`sum < 1.0`, common in Tripo3D/glTF exports) cause vertices to be pulled
/// toward world origin `(0, 0, 0)` proportionally to `(1.0 - sum) * entity_world_position`, creating
/// long diagonal vertex-stretch spikes whenever the unit is far from map origin.
pub fn normalize_joint_weight(w: [f32; 4]) -> ([f32; 4], bool) {
    let mut clean = [0.0_f32; 4];
    let mut has_invalid = false;
    for i in 0..4 {
        let c = w[i];
        if c.is_finite() && c >= 0.0 {
            clean[i] = c;
        } else {
            has_invalid = true;
        }
    }

    let sum = clean[0] + clean[1] + clean[2] + clean[3];
    if sum <= 1e-8 {
        return ([1.0, 0.0, 0.0, 0.0], true);
    }

    if !has_invalid && (sum - 1.0).abs() <= WEIGHT_SUM_EPSILON {
        return (clean, false);
    }

    let inv = 1.0 / sum;
    for c in &mut clean {
        *c *= inv;
    }

    // Snap the largest component so the 4 weights sum to exact 1.0_f32 (zero drift at large offsets)
    let mut max_i = 0;
    for i in 1..4 {
        if clean[i] > clean[max_i] {
            max_i = i;
        }
    }
    let others: f32 = (0..4)
        .filter(|&i| i != max_i)
        .map(|i| clean[i])
        .sum();
    clean[max_i] = (1.0 - others).max(0.0);
    (clean, true)
}

/// Normalizes quantized integer `WEIGHTS_0` (`UNSIGNED_BYTE` max=255 or `UNSIGNED_SHORT` max=65535)
/// using largest-remainder rounding so the 4 components sum to exact `MAX_VAL`.
fn normalize_quantized_weight<const MAX_VAL: u32>(raw: [u32; 4]) -> ([u32; 4], bool) {
    let raw_sum: u32 = raw.iter().copied().sum();
    if raw_sum == MAX_VAL {
        return (raw, false);
    }
    if raw_sum == 0 {
        return ([MAX_VAL, 0, 0, 0], true);
    }

    let scale = MAX_VAL as f64 / raw_sum as f64;
    let mut floors = [0u32; 4];
    let mut fracs = [(0usize, 0.0f64); 4];
    let mut floor_sum = 0u32;
    for i in 0..4 {
        let exact = raw[i] as f64 * scale;
        let fl = exact.floor() as u32;
        floors[i] = fl;
        fracs[i] = (i, exact - fl as f64);
        floor_sum += fl;
    }

    let mut rem = MAX_VAL.saturating_sub(floor_sum);
    fracs.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (idx, _) in fracs {
        if rem == 0 {
            break;
        }
        floors[idx] += 1;
        rem -= 1;
    }

    (floors, floors != raw)
}

/// Inspects a binary `.glb` (glTF 2.0) buffer and normalizes any unnormalized `WEIGHTS_0`
/// skinning attributes in-place.
///
/// - If the `.glb` is not skinned or all `WEIGHTS_0` vectors already sum to `1.0`, returns
///   `Ok((None, GlbSkinNormalizeReport::default()))` without allocating an output copy.
/// - If any vertices were repaired, returns `Ok((Some(repaired_bytes), report))`.
pub fn normalize_glb_joint_weights(
    bytes: &[u8],
) -> Result<(Option<Vec<u8>>, GlbSkinNormalizeReport), String> {
    if bytes.len() < 20 || &bytes[0..4] != GLB_MAGIC {
        return Err("Invalid GLB header magic".into());
    }
    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if version != 2 {
        return Err(format!("Unsupported GLB version: {version}"));
    }

    let json_len = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    let json_type = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    if json_type != GLB_CHUNK_JSON {
        return Err("First GLB chunk is not JSON".into());
    }
    let json_end = 20usize
        .checked_add(json_len)
        .filter(|&end| end <= bytes.len())
        .ok_or_else(|| "Truncated GLB JSON chunk".to_string())?;

    let mut json: Value = serde_json::from_slice(&bytes[20..json_end])
        .map_err(|e| format!("Invalid GLB JSON chunk: {e}"))?;

    // Collect every unique accessor index used as WEIGHTS_0 on any mesh primitive.
    let mut weight_accessors: Vec<usize> = Vec::new();
    let mut seen_accessors: HashSet<usize> = HashSet::new();
    if let Some(meshes) = json.get("meshes").and_then(Value::as_array) {
        for mesh in meshes {
            if let Some(primitives) = mesh.get("primitives").and_then(Value::as_array) {
                for prim in primitives {
                    if let Some(acc_idx) = prim
                        .get("attributes")
                        .and_then(|a| a.get("WEIGHTS_0"))
                        .and_then(Value::as_u64)
                        .map(|n| n as usize)
                        && seen_accessors.insert(acc_idx)
                    {
                        weight_accessors.push(acc_idx);
                    }
                }
            }
        }
    }

    if weight_accessors.is_empty() {
        return Ok((None, GlbSkinNormalizeReport::default()));
    }

    // Locate the binary chunk (chunk 1).
    if json_end + 8 > bytes.len() {
        return Ok((None, GlbSkinNormalizeReport::default()));
    }
    let bin_len = u32::from_le_bytes([
        bytes[json_end],
        bytes[json_end + 1],
        bytes[json_end + 2],
        bytes[json_end + 3],
    ]) as usize;
    let bin_type = u32::from_le_bytes([
        bytes[json_end + 4],
        bytes[json_end + 5],
        bytes[json_end + 6],
        bytes[json_end + 7],
    ]);
    if bin_type != GLB_CHUNK_BIN {
        return Ok((None, GlbSkinNormalizeReport::default()));
    }
    let bin_start = json_end + 8;
    let bin_end = bin_start
        .checked_add(bin_len)
        .filter(|&end| end <= bytes.len())
        .ok_or_else(|| "Truncated GLB BIN chunk".to_string())?;

    let mut patched: Option<Vec<u8>> = None;
    let mut report = GlbSkinNormalizeReport::default();
    let mut json_minmax_updates: Vec<(usize, [f64; 4], [f64; 4])> = Vec::new();

    for acc_idx in weight_accessors {
        let Some(acc) = json
            .get("accessors")
            .and_then(Value::as_array)
            .and_then(|arr| arr.get(acc_idx))
        else {
            continue;
        };

        // Only handle standard non-sparse VEC4 accessors backed by buffer 0 (GLB BIN chunk).
        if acc.get("sparse").is_some() {
            continue;
        }
        if acc.get("type").and_then(Value::as_str) != Some("VEC4") {
            continue;
        }
        let Some(comp_type) = acc.get("componentType").and_then(Value::as_u64) else {
            continue;
        };
        let Some(count) = acc.get("count").and_then(Value::as_u64).map(|n| n as usize) else {
            continue;
        };
        if count == 0 {
            continue;
        }
        let Some(bv_idx) = acc
            .get("bufferView")
            .and_then(Value::as_u64)
            .map(|n| n as usize)
        else {
            continue;
        };
        let acc_offset = acc
            .get("byteOffset")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;

        let Some(bv) = json
            .get("bufferViews")
            .and_then(Value::as_array)
            .and_then(|arr| arr.get(bv_idx))
        else {
            continue;
        };
        if bv.get("buffer").and_then(Value::as_u64).unwrap_or(0) != 0 {
            continue;
        }
        let bv_offset = bv
            .get("byteOffset")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let elem_size = match comp_type {
            COMPONENT_FLOAT => 16usize,
            COMPONENT_UNSIGNED_SHORT => 8usize,
            COMPONENT_UNSIGNED_BYTE => 4usize,
            _ => continue,
        };
        let stride = bv
            .get("byteStride")
            .and_then(Value::as_u64)
            .map(|s| s as usize)
            .unwrap_or(elem_size);
        if stride < elem_size {
            continue;
        }

        let base_rel = match bv_offset.checked_add(acc_offset) {
            Some(v) => v,
            None => continue,
        };
        let last_end_rel = match (count - 1)
            .checked_mul(stride)
            .and_then(|o| base_rel.checked_add(o))
            .and_then(|o| o.checked_add(elem_size))
        {
            Some(v) if v <= bin_len => v,
            _ => continue,
        };
        let _ = last_end_rel;

        let has_minmax = acc.get("min").is_some() || acc.get("max").is_some();
        let mut acc_repaired = 0usize;
        let mut min_vals = [f64::INFINITY; 4];
        let mut max_vals = [f64::NEG_INFINITY; 4];

        for i in 0..count {
            let off = bin_start + base_rel + i * stride;
            let src = patched.as_deref().unwrap_or(bytes);
            match comp_type {
                COMPONENT_FLOAT => {
                    let w = [
                        f32::from_le_bytes(src[off..off + 4].try_into().unwrap()),
                        f32::from_le_bytes(src[off + 4..off + 8].try_into().unwrap()),
                        f32::from_le_bytes(src[off + 8..off + 12].try_into().unwrap()),
                        f32::from_le_bytes(src[off + 12..off + 16].try_into().unwrap()),
                    ];
                    let (norm, changed) = normalize_joint_weight(w);
                    if changed {
                        let out = patched.get_or_insert_with(|| bytes.to_vec());
                        for k in 0..4 {
                            out[off + k * 4..off + (k + 1) * 4]
                                .copy_from_slice(&norm[k].to_le_bytes());
                        }
                        acc_repaired += 1;
                    }
                    if has_minmax {
                        for k in 0..4 {
                            let v = norm[k] as f64;
                            min_vals[k] = min_vals[k].min(v);
                            max_vals[k] = max_vals[k].max(v);
                        }
                    }
                }
                COMPONENT_UNSIGNED_SHORT => {
                    let raw = [
                        u16::from_le_bytes(src[off..off + 2].try_into().unwrap()) as u32,
                        u16::from_le_bytes(src[off + 2..off + 4].try_into().unwrap()) as u32,
                        u16::from_le_bytes(src[off + 4..off + 6].try_into().unwrap()) as u32,
                        u16::from_le_bytes(src[off + 6..off + 8].try_into().unwrap()) as u32,
                    ];
                    let (norm, changed) = normalize_quantized_weight::<65535>(raw);
                    if changed {
                        let out = patched.get_or_insert_with(|| bytes.to_vec());
                        for k in 0..4 {
                            out[off + k * 2..off + (k + 1) * 2]
                                .copy_from_slice(&(norm[k] as u16).to_le_bytes());
                        }
                        acc_repaired += 1;
                    }
                    if has_minmax {
                        for k in 0..4 {
                            let v = norm[k] as f64;
                            min_vals[k] = min_vals[k].min(v);
                            max_vals[k] = max_vals[k].max(v);
                        }
                    }
                }
                COMPONENT_UNSIGNED_BYTE => {
                    let raw = [
                        src[off] as u32,
                        src[off + 1] as u32,
                        src[off + 2] as u32,
                        src[off + 3] as u32,
                    ];
                    let (norm, changed) = normalize_quantized_weight::<255>(raw);
                    if changed {
                        let out = patched.get_or_insert_with(|| bytes.to_vec());
                        for k in 0..4 {
                            out[off + k] = norm[k] as u8;
                        }
                        acc_repaired += 1;
                    }
                    if has_minmax {
                        for k in 0..4 {
                            let v = norm[k] as f64;
                            min_vals[k] = min_vals[k].min(v);
                            max_vals[k] = max_vals[k].max(v);
                        }
                    }
                }
                _ => {}
            }
        }

        if acc_repaired > 0 {
            report.repaired_vertices += acc_repaired;
            report.repaired_accessors += 1;
            if has_minmax {
                json_minmax_updates.push((acc_idx, min_vals, max_vals));
            }
        }
    }

    let Some(mut out_bytes) = patched else {
        return Ok((None, report));
    };

    // If any modified accessor had explicit JSON min/max bounds, update them and rebuild the
    // JSON chunk cleanly so glTF validators remain 100% happy.
    if !json_minmax_updates.is_empty()
        && let Some(accessors) = json.get_mut("accessors").and_then(Value::as_array_mut)
    {
        for (idx, mins, maxs) in json_minmax_updates {
            if let Some(acc_obj) = accessors.get_mut(idx).and_then(Value::as_object_mut) {
                if acc_obj.contains_key("min") {
                    acc_obj.insert("min".into(), serde_json::json!(mins));
                }
                if acc_obj.contains_key("max") {
                    acc_obj.insert("max".into(), serde_json::json!(maxs));
                }
            }
        }
        let mut new_json = serde_json::to_vec(&json)
            .map_err(|e| format!("Failed serializing updated GLB JSON: {e}"))?;
        while !new_json.len().is_multiple_of(4) {
            new_json.push(b' ');
        }
        let tail = &out_bytes[json_end..];
        let total_len = 20 + new_json.len() + tail.len();
        let mut rebuilt = Vec::with_capacity(total_len);
        rebuilt.extend_from_slice(GLB_MAGIC);
        rebuilt.extend_from_slice(&2u32.to_le_bytes());
        rebuilt.extend_from_slice(&(total_len as u32).to_le_bytes());
        rebuilt.extend_from_slice(&(new_json.len() as u32).to_le_bytes());
        rebuilt.extend_from_slice(&GLB_CHUNK_JSON.to_le_bytes());
        rebuilt.extend_from_slice(&new_json);
        rebuilt.extend_from_slice(tail);
        out_bytes = rebuilt;
    }

    let _ = bin_end;
    Ok((Some(out_bytes), report))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_minimal_glb(json_val: &Value, bin_payload: &[u8]) -> Vec<u8> {
        let mut json_bytes = serde_json::to_vec(json_val).unwrap();
        while !json_bytes.len().is_multiple_of(4) {
            json_bytes.push(b' ');
        }
        let mut bin_bytes = bin_payload.to_vec();
        while !bin_bytes.len().is_multiple_of(4) {
            bin_bytes.push(0);
        }
        let total_len = 12 + 8 + json_bytes.len() + 8 + bin_bytes.len();
        let mut glb = Vec::with_capacity(total_len);
        glb.extend_from_slice(GLB_MAGIC);
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total_len as u32).to_le_bytes());
        glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        glb.extend_from_slice(&GLB_CHUNK_JSON.to_le_bytes());
        glb.extend_from_slice(&json_bytes);
        glb.extend_from_slice(&(bin_bytes.len() as u32).to_le_bytes());
        glb.extend_from_slice(&GLB_CHUNK_BIN.to_le_bytes());
        glb.extend_from_slice(&bin_bytes);
        glb
    }

    #[test]
    fn normalize_joint_weight_handles_brimob_babin_zero_nan_and_valid() {
        // brimob_walk.glb v8730 (sum = 0.92791225)
        let (norm, changed) =
            normalize_joint_weight([0.6743563, 0.1148743, 0.07808124, 0.06060036]);
        assert!(changed);
        assert!((norm.iter().sum::<f32>() - 1.0).abs() <= 1e-6);

        // babin_walk.glb v4965 (sum = 0.9544032)
        let (norm2, changed2) =
            normalize_joint_weight([0.800534, 0.07298717, 0.06364223, 0.01723978]);
        assert!(changed2);
        assert!((norm2.iter().sum::<f32>() - 1.0).abs() <= 1e-6);

        // All-zero weight binds cleanly to joint 0
        assert_eq!(
            normalize_joint_weight([0.0, 0.0, 0.0, 0.0]),
            ([1.0, 0.0, 0.0, 0.0], true)
        );

        // Negative / NaN sanitized
        let (san, san_changed) = normalize_joint_weight([-0.5, f32::NAN, 0.3, 0.3]);
        assert!(san_changed);
        assert_eq!(san[0], 0.0);
        assert_eq!(san[1], 0.0);
        assert!((san.iter().sum::<f32>() - 1.0).abs() <= 1e-6);

        // Already normalized untouched
        assert_eq!(
            normalize_joint_weight([0.6, 0.4, 0.0, 0.0]),
            ([0.6, 0.4, 0.0, 0.0], false)
        );
    }

    #[test]
    fn normalize_glb_f32_weights_in_place_and_idempotent() {
        let weights: [[f32; 4]; 3] = [
            [0.800534, 0.07298717, 0.06364223, 0.01723978], // babin_walk.glb v4965
            [0.5, 0.5, 0.0, 0.0],                           // already valid
            [0.0, 0.0, 0.0, 0.0],                           // zero weight
        ];
        let mut bin = Vec::new();
        for w in weights {
            for c in w {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        let json = serde_json::json!({
            "asset": { "version": "2.0" },
            "buffers": [{ "byteLength": bin.len() }],
            "bufferViews": [{ "buffer": 0, "byteOffset": 0, "byteLength": bin.len() }],
            "accessors": [{
                "bufferView": 0,
                "componentType": 5126,
                "count": 3,
                "type": "VEC4"
            }],
            "meshes": [{
                "primitives": [{
                    "attributes": { "WEIGHTS_0": 0 }
                }]
            }]
        });
        let glb = build_minimal_glb(&json, &bin);

        let (patched_opt, rep) = normalize_glb_joint_weights(&glb).expect("normalize ok");
        assert_eq!(rep.repaired_accessors, 1);
        assert_eq!(rep.repaired_vertices, 2);
        let patched = patched_opt.expect("patched bytes produced");
        assert_eq!(patched.len(), glb.len());

        // Second pass must be a zero-copy no-op (idempotent!)
        let (second_opt, rep2) = normalize_glb_joint_weights(&patched).expect("2nd pass ok");
        assert!(second_opt.is_none());
        assert_eq!(rep2.repaired_vertices, 0);
    }

    #[test]
    fn normalize_glb_u8_and_u16_weights_exact_sum() {
        // UNSIGNED_BYTE: [200, 20, 10, 5] sums to 235 != 255
        let bin_u8 = vec![200u8, 20, 10, 5, 255, 0, 0, 0];
        let json_u8 = serde_json::json!({
            "asset": { "version": "2.0" },
            "buffers": [{ "byteLength": bin_u8.len() }],
            "bufferViews": [{ "buffer": 0, "byteOffset": 0, "byteLength": bin_u8.len() }],
            "accessors": [{
                "bufferView": 0,
                "componentType": 5121,
                "normalized": true,
                "count": 2,
                "type": "VEC4"
            }],
            "meshes": [{ "primitives": [{ "attributes": { "WEIGHTS_0": 0 } }] }]
        });
        let glb_u8 = build_minimal_glb(&json_u8, &bin_u8);
        let (patched_u8, rep_u8) = normalize_glb_joint_weights(&glb_u8).unwrap();
        assert_eq!(rep_u8.repaired_vertices, 1);
        let patched_u8 = patched_u8.unwrap();
        assert!(normalize_glb_joint_weights(&patched_u8).unwrap().0.is_none());

        // UNSIGNED_SHORT: [50000, 10000, 0, 0] sums to 60000 != 65535
        let mut bin_u16 = Vec::new();
        for v in [50000u16, 10000, 0, 0] {
            bin_u16.extend_from_slice(&v.to_le_bytes());
        }
        let json_u16 = serde_json::json!({
            "asset": { "version": "2.0" },
            "buffers": [{ "byteLength": bin_u16.len() }],
            "bufferViews": [{ "buffer": 0, "byteOffset": 0, "byteLength": bin_u16.len() }],
            "accessors": [{
                "bufferView": 0,
                "componentType": 5123,
                "normalized": true,
                "count": 1,
                "type": "VEC4"
            }],
            "meshes": [{ "primitives": [{ "attributes": { "WEIGHTS_0": 0 } }] }]
        });
        let glb_u16 = build_minimal_glb(&json_u16, &bin_u16);
        let (patched_u16, rep_u16) = normalize_glb_joint_weights(&glb_u16).unwrap();
        assert_eq!(rep_u16.repaired_vertices, 1);
        let patched_u16 = patched_u16.unwrap();
        assert!(normalize_glb_joint_weights(&patched_u16).unwrap().0.is_none());
    }

    #[test]
    fn normalize_glb_skips_non_skinned_models_without_copy() {
        let json = serde_json::json!({
            "asset": { "version": "2.0" },
            "meshes": [{ "primitives": [{ "attributes": { "POSITION": 0 } }] }]
        });
        let glb = build_minimal_glb(&json, &[0u8; 12]);
        let (res, rep) = normalize_glb_joint_weights(&glb).unwrap();
        assert!(res.is_none());
        assert_eq!(rep.repaired_vertices, 0);
    }
}
