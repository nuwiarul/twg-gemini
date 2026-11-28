use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn parse_env_file(path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(content) = std::fs::read_to_string(path) else {
        return map;
    };
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim().to_string();
        let mut val = v.trim().to_string();
        if val.len() >= 2
            && ((val.starts_with('"') && val.ends_with('"'))
                || (val.starts_with('\'') && val.ends_with('\'')))
        {
            val = val[1..val.len() - 1].to_string();
        }
        if !key.is_empty() {
            map.insert(key, val);
        }
    }
    map
}

fn resolve_path_from_repo(repo_root: &Path, raw: &str) -> PathBuf {
    let p = PathBuf::from(raw);
    if p.is_absolute() {
        p
    } else {
        repo_root.join(p)
    }
}

fn resolve_brand_value(
    env_key: &str,
    file_key: &str,
    file_map: &HashMap<String, String>,
    default_val: &str,
) -> String {
    if let Ok(v) = std::env::var(env_key) {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if let Some(v) = file_map.get(file_key) {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    default_val.to_string()
}

fn clean_path_for_include(path: &Path) -> String {
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let s = abs.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        stripped.to_string()
    } else {
        s.into_owned()
    }
}

fn main() {
    println!("cargo:rerun-if-changed=../../packaging/icons/twg-app.ico");
    for env_key in [
        "TWG_BRAND",
        "TWG_BRAND_DIR",
        "TWG_BRAND_LOGOS_DIR",
        "TWG_PUBLIC_SERVER_URL",
        "TWG_ORG_NAME",
        "TWG_ORG_SUBTITLE",
        "TWG_APP_TITLE",
    ] {
        println!("cargo:rerun-if-env-changed={env_key}");
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| manifest_dir.clone());

    let brands_root = repo_root.join("packaging").join("brands");
    println!("cargo:rerun-if-changed={}", brands_root.display());

    let explicit_brand_dir = std::env::var("TWG_BRAND_DIR")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let explicit_brand_name = std::env::var("TWG_BRAND")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let brand_dir = if let Some(raw_dir) = &explicit_brand_dir {
        resolve_path_from_repo(&repo_root, raw_dir)
    } else if let Some(name) = &explicit_brand_name {
        brands_root.join(name)
    } else {
        let default_bali = brands_root.join("polda-bali");
        if default_bali.is_dir() {
            default_bali
        } else {
            repo_root.join("packaging").join("brand")
        }
    };

    if (explicit_brand_dir.is_some() || explicit_brand_name.is_some()) && !brand_dir.is_dir() {
        println!(
            "cargo:warning=TWG brand profile folder not found: {}",
            brand_dir.display()
        );
    }

    let brand_env_path = brand_dir.join("brand.env");
    println!("cargo:rerun-if-changed={}", brand_dir.display());
    println!("cargo:rerun-if-changed={}", brand_env_path.display());

    let file_map = parse_env_file(&brand_env_path);

    let public_server_url = resolve_brand_value(
        "TWG_PUBLIC_SERVER_URL",
        "PUBLIC_SERVER_URL",
        &file_map,
        "",
    )
    .trim_end_matches('/')
    .to_string();
    let org_name = resolve_brand_value("TWG_ORG_NAME", "ORG_NAME", &file_map, "");
    let org_subtitle = resolve_brand_value("TWG_ORG_SUBTITLE", "ORG_SUBTITLE", &file_map, "");
    let app_title = resolve_brand_value(
        "TWG_APP_TITLE",
        "APP_TITLE",
        &file_map,
        "Tactical Wall Game 3D",
    );

    let logos_dir = std::env::var("TWG_BRAND_LOGOS_DIR")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|raw| resolve_path_from_repo(&repo_root, &raw))
        .unwrap_or_else(|| brand_dir.join("logos"));

    println!("cargo:rerun-if-changed={}", logos_dir.display());

    let mut logo_paths: Vec<String> = Vec::new();
    for idx in 1..=3 {
        let candidate = logos_dir.join(format!("{idx}.png"));
        println!("cargo:rerun-if-changed={}", candidate.display());
        if candidate.is_file() {
            logo_paths.push(clean_path_for_include(&candidate));
        }
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set"));
    let generated_path = out_dir.join("brand_config.rs");

    let mut code = String::new();
    code.push_str("// Auto-generated by crates/twg-app/build.rs — do not edit manually.\n");
    code.push_str(&format!(
        "pub const PUBLIC_SERVER_URL: &str = {:?};\n",
        public_server_url
    ));
    code.push_str(&format!("pub const ORG_NAME: &str = {:?};\n", org_name));
    code.push_str(&format!(
        "pub const ORG_SUBTITLE: &str = {:?};\n",
        org_subtitle
    ));
    code.push_str(&format!("pub const APP_TITLE: &str = {:?};\n", app_title));
    code.push_str("pub const BRAND_LOGOS_PNG: &[&[u8]] = &[\n");
    for lp in &logo_paths {
        code.push_str(&format!("    include_bytes!({:?}),\n", lp));
    }
    code.push_str("];\n");

    std::fs::write(&generated_path, code).expect("Failed to write generated brand_config.rs");

    #[cfg(target_os = "windows")]
    {
        let icon_path = "../../packaging/icons/twg-app.ico";
        if std::path::Path::new(icon_path).exists() {
            let mut res = winresource::WindowsResource::new();
            res.set_icon(icon_path);
            let product_name = if org_name.is_empty() {
                app_title.clone()
            } else {
                format!("{app_title} — {org_name}")
            };
            res.set("ProductName", &product_name);
            res.set(
                "FileDescription",
                &format!("{product_name} — Tactical Simulation Desktop Client"),
            );
            res.set(
                "CompanyName",
                if org_name.is_empty() {
                    "TWG Team"
                } else {
                    &org_name
                },
            );
            if let Err(err) = res.compile() {
                println!("cargo:warning=Failed to embed Windows icon resource: {err}");
            }
        }
    }
}
