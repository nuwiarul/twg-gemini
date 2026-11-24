use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;
use twg_shared::{
    api::ApiResponse,
    models::{ModelAssetDto, ModelFileMeta, ModelSyncManifest},
};

const MAX_CONCURRENT_DOWNLOADS: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalSyncCache {
    pub manifest: ModelSyncManifest,
    /// rel_path -> (sha256, size_bytes)
    pub verified_files: HashMap<String, (String, i64)>,
}

#[derive(Debug, Clone)]
pub enum AssetSyncCommand {
    SyncWithServer {
        server_url: String,
        local_root: PathBuf,
    },
    PriorityDownload {
        server_url: String,
        local_root: PathBuf,
        files: Vec<ModelFileMeta>,
    },
}

#[derive(Debug, Clone)]
pub enum AssetSyncEvent {
    Checking {
        server_url: String,
    },
    ManifestLoaded {
        manifest: ModelSyncManifest,
        to_download: usize,
        to_delete: usize,
        download_bytes: u64,
        already_synced: usize,
    },
    Progress {
        completed_files: usize,
        total_files: usize,
        downloaded_bytes: u64,
        total_bytes: u64,
        current_file: String,
    },
    FileSynced {
        rel_path: String,
        was_updated: bool,
    },
    FileRemoved {
        rel_path: String,
    },
    Completed {
        manifest: ModelSyncManifest,
        downloaded: usize,
        updated: usize,
        deleted: usize,
        skipped: usize,
    },
    OfflineFallback {
        reason: String,
        cached_manifest: Option<ModelSyncManifest>,
    },
}

#[derive(Resource)]
pub struct AssetSyncBridge {
    cmd_tx: Sender<AssetSyncCommand>,
    evt_rx: Receiver<AssetSyncEvent>,
}

impl AssetSyncBridge {
    pub fn start() -> Self {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded::<AssetSyncCommand>();
        let (evt_tx, evt_rx) = crossbeam_channel::unbounded::<AssetSyncEvent>();

        std::thread::Builder::new()
            .name("twg-asset-sync".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(3)
                    .enable_all()
                    .build()
                    .expect("failed to build asset sync tokio runtime");

                let client = Client::builder()
                    .timeout(std::time::Duration::from_secs(60))
                    .build()
                    .expect("failed to build asset sync reqwest client");

                let active_sync = Arc::new(tokio::sync::Mutex::new(()));

                rt.block_on(async move {
                    while let Ok(cmd) = cmd_rx.recv() {
                        let c = client.clone();
                        let tx = evt_tx.clone();
                        let lock = Arc::clone(&active_sync);
                        match cmd {
                            AssetSyncCommand::SyncWithServer {
                                server_url,
                                local_root,
                            } => {
                                tokio::spawn(async move {
                                    let _guard = lock.lock().await;
                                    run_full_sync(&c, &server_url, &local_root, &tx).await;
                                });
                            }
                            AssetSyncCommand::PriorityDownload {
                                server_url,
                                local_root,
                                files,
                            } => {
                                tokio::spawn(async move {
                                    run_priority_downloads(&c, &server_url, &local_root, files, &tx)
                                        .await;
                                });
                            }
                        }
                    }
                });
            })
            .expect("failed to spawn asset sync worker thread");

        Self { cmd_tx, evt_rx }
    }

    pub fn send(&self, cmd: AssetSyncCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn drain_events(&self) -> Vec<AssetSyncEvent> {
        self.evt_rx.try_iter().collect()
    }
}

pub fn load_cached_sync_manifest(local_root: &Path) -> Option<LocalSyncCache> {
    let path = local_root.join("sync_manifest.json");
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn scan_existing_local_models(local_root: &Path) -> HashSet<String> {
    let mut out = HashSet::new();
    collect_asset_rel_paths(local_root, &local_root.join("models"), &mut out);
    collect_asset_rel_paths(local_root, &local_root.join("media"), &mut out);
    out
}

fn collect_asset_rel_paths(root: &Path, dir: &Path, out: &mut HashSet<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_asset_rel_paths(root, &p, out);
        } else if p
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| {
                !e.eq_ignore_ascii_case("part") && !e.eq_ignore_ascii_case("tmp")
            })
            && let Ok(rel) = p.strip_prefix(root)
        {
            let norm = rel.to_string_lossy().replace('\\', "/");
            out.insert(norm);
        }
    }
}

async fn run_priority_downloads(
    client: &Client,
    server_url: &str,
    local_root: &Path,
    files: Vec<ModelFileMeta>,
    evt_tx: &Sender<AssetSyncEvent>,
) {
    let base = server_url.trim_end_matches('/');
    for meta in files {
        let dst = local_root.join(&meta.rel_path);
        if file_matches_meta(&dst, &meta, None).await {
            let _ = evt_tx.send(AssetSyncEvent::FileSynced {
                rel_path: meta.rel_path,
                was_updated: false,
            });
            continue;
        }
        let existed = dst.exists();
        if download_and_verify(client, base, local_root, &meta).await.is_ok() {
            let _ = evt_tx.send(AssetSyncEvent::FileSynced {
                rel_path: meta.rel_path,
                was_updated: existed,
            });
        }
    }
}

async fn run_full_sync(
    client: &Client,
    server_url: &str,
    local_root: &Path,
    evt_tx: &Sender<AssetSyncEvent>,
) {
    let base = server_url.trim_end_matches('/');
    let _ = evt_tx.send(AssetSyncEvent::Checking {
        server_url: base.to_string(),
    });

    let cached = load_cached_sync_manifest(local_root);
    let manifest_url = format!("{base}/api/v1/assets/models/manifest");

    let resp = match client.get(&manifest_url).send().await {
        Ok(r) => r,
        Err(err) => {
            let _ = evt_tx.send(AssetSyncEvent::OfflineFallback {
                reason: format!("Server tidak terjangkau ({err}) — menggunakan cache lokal"),
                cached_manifest: cached.map(|c| c.manifest),
            });
            return;
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let _ = evt_tx.send(AssetSyncEvent::OfflineFallback {
            reason: format!("Manifest HTTP {status} — menggunakan cache lokal"),
            cached_manifest: cached.map(|c| c.manifest),
        });
        return;
    }

    let text = match resp.text().await {
        Ok(t) => t,
        Err(err) => {
            let _ = evt_tx.send(AssetSyncEvent::OfflineFallback {
                reason: format!("Gagal membaca manifest ({err}) — menggunakan cache lokal"),
                cached_manifest: cached.map(|c| c.manifest),
            });
            return;
        }
    };

    let manifest = match serde_json::from_str::<ApiResponse<ModelSyncManifest>>(&text) {
        Ok(wrapped) => wrapped.data,
        Err(err) => {
            let _ = evt_tx.send(AssetSyncEvent::OfflineFallback {
                reason: format!("Format manifest tidak valid ({err})"),
                cached_manifest: cached.map(|c| c.manifest),
            });
            return;
        }
    };

    let _ = tokio::fs::create_dir_all(local_root.join("models")).await;
    let _ = tokio::fs::create_dir_all(local_root.join("media")).await;

    let cached_hashes = cached
        .as_ref()
        .map(|c| c.verified_files.clone())
        .unwrap_or_default();

    // Index all expected files from server manifest (3D models + textures + sounds + videos)
    let mut expected_files: HashMap<String, ModelFileMeta> = HashMap::new();
    for m in &manifest.models {
        for f in m.all_files() {
            expected_files.insert(f.rel_path.clone(), f.clone());
        }
    }
    for ma in &manifest.media_assets {
        for f in ma.all_files() {
            expected_files.insert(f.rel_path.clone(), f.clone());
        }
    }

    // Delete local model files removed from server
    let existing_local = scan_existing_local_models(local_root);
    let mut deleted_count = 0usize;
    for local_rel in &existing_local {
        if !expected_files.contains_key(local_rel) {
            let abs = local_root.join(local_rel);
            if tokio::fs::remove_file(&abs).await.is_ok() {
                deleted_count += 1;
                let _ = evt_tx.send(AssetSyncEvent::FileRemoved {
                    rel_path: local_rel.clone(),
                });
            }
        }
    }

    // Determine which files are missing or modified on server
    let mut download_queue: Vec<(ModelFileMeta, bool)> = Vec::new();
    let mut verified_map: HashMap<String, (String, i64)> = HashMap::new();
    let mut skipped_count = 0usize;
    let mut total_download_bytes = 0u64;

    for (rel_path, meta) in &expected_files {
        let abs = local_root.join(rel_path);
        let cached_entry = cached_hashes.get(rel_path);
        if file_matches_meta(&abs, meta, cached_entry).await {
            skipped_count += 1;
            verified_map.insert(rel_path.clone(), (meta.sha256.clone(), meta.size_bytes));
        } else {
            let existed = abs.exists();
            total_download_bytes += meta.size_bytes.max(0) as u64;
            download_queue.push((meta.clone(), existed));
        }
    }

    // Sort queue so LOD0 primary files download cleanly in category order
    download_queue.sort_by(|a, b| a.0.rel_path.cmp(&b.0.rel_path));

    let total_to_download = download_queue.len();
    let _ = evt_tx.send(AssetSyncEvent::ManifestLoaded {
        manifest: manifest.clone(),
        to_download: total_to_download,
        to_delete: deleted_count,
        download_bytes: total_download_bytes,
        already_synced: skipped_count,
    });

    let completed = Arc::new(AtomicUsize::new(0));
    let downloaded_bytes = Arc::new(AtomicU64::new(0));
    let downloaded_new = Arc::new(AtomicUsize::new(0));
    let updated_existing = Arc::new(AtomicUsize::new(0));
    let verified_shared = Arc::new(tokio::sync::Mutex::new(verified_map));
    let sem = Arc::new(Semaphore::new(MAX_CONCURRENT_DOWNLOADS));

    let mut join_set = tokio::task::JoinSet::new();
    for (meta, was_updated) in download_queue {
        let client = client.clone();
        let base = base.to_string();
        let local_root = local_root.to_path_buf();
        let evt_tx = evt_tx.clone();
        let sem = Arc::clone(&sem);
        let completed = Arc::clone(&completed);
        let downloaded_bytes = Arc::clone(&downloaded_bytes);
        let downloaded_new = Arc::clone(&downloaded_new);
        let updated_existing = Arc::clone(&updated_existing);
        let verified_shared = Arc::clone(&verified_shared);

        join_set.spawn(async move {
            let _permit = sem.acquire().await.ok();
            let res = download_and_verify(&client, &base, &local_root, &meta).await;
            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
            if res.is_ok() {
                let bytes_now = downloaded_bytes
                    .fetch_add(meta.size_bytes.max(0) as u64, Ordering::Relaxed)
                    + meta.size_bytes.max(0) as u64;
                if was_updated {
                    updated_existing.fetch_add(1, Ordering::Relaxed);
                } else {
                    downloaded_new.fetch_add(1, Ordering::Relaxed);
                }
                {
                    let mut map = verified_shared.lock().await;
                    map.insert(
                        meta.rel_path.clone(),
                        (meta.sha256.clone(), meta.size_bytes),
                    );
                }
                let _ = evt_tx.send(AssetSyncEvent::FileSynced {
                    rel_path: meta.rel_path.clone(),
                    was_updated,
                });
                let _ = evt_tx.send(AssetSyncEvent::Progress {
                    completed_files: done,
                    total_files: total_to_download,
                    downloaded_bytes: bytes_now,
                    total_bytes: total_download_bytes,
                    current_file: meta.rel_path,
                });
            } else {
                let bytes_now = downloaded_bytes.load(Ordering::Relaxed);
                let _ = evt_tx.send(AssetSyncEvent::Progress {
                    completed_files: done,
                    total_files: total_to_download,
                    downloaded_bytes: bytes_now,
                    total_bytes: total_download_bytes,
                    current_file: format!("failed: {}", meta.rel_path),
                });
            }
        });
    }

    while join_set.join_next().await.is_some() {}

    let final_verified = verified_shared.lock().await.clone();
    let cache = LocalSyncCache {
        manifest: manifest.clone(),
        verified_files: final_verified,
    };
    let _ = save_local_sync_cache(local_root, &cache).await;

    let _ = evt_tx.send(AssetSyncEvent::Completed {
        manifest,
        downloaded: downloaded_new.load(Ordering::Relaxed),
        updated: updated_existing.load(Ordering::Relaxed),
        deleted: deleted_count,
        skipped: skipped_count,
    });
}

async fn file_matches_meta(
    abs_path: &Path,
    meta: &ModelFileMeta,
    cached_entry: Option<&(String, i64)>,
) -> bool {
    let Ok(file_meta) = tokio::fs::metadata(abs_path).await else {
        return false;
    };
    let actual_len = file_meta.len() as i64;
    if meta.size_bytes > 0 && actual_len != meta.size_bytes {
        return false;
    }

    // Fast path: if previously verified hash + size match server manifest, skip re-hashing
    if let Some((cached_sha, cached_sz)) = cached_entry
        && cached_sha == &meta.sha256
        && *cached_sz == actual_len
    {
        return true;
    }

    // Otherwise compute actual SHA-256 on disk to ensure exact integrity
    let Ok(bytes) = tokio::fs::read(abs_path).await else {
        return false;
    };
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let actual_sha = hex::encode(hasher.finalize());
    actual_sha == meta.sha256
}

async fn download_and_verify(
    client: &Client,
    base_url: &str,
    local_root: &Path,
    meta: &ModelFileMeta,
) -> Result<(), String> {
    let url = format!("{base_url}{}", meta.download_url);
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("download failed for {}: {e}", meta.rel_path))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} for {}", resp.status(), meta.rel_path));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("read body failed for {}: {e}", meta.rel_path))?;

    if !meta.sha256.is_empty() {
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let got = hex::encode(hasher.finalize());
        if got != meta.sha256 {
            return Err(format!(
                "SHA-256 mismatch for {}: expected {}, got {}",
                meta.rel_path, meta.sha256, got
            ));
        }
    }

    let target = local_root.join(&meta.rel_path);
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("mkdir failed: {e}"))?;
    }
    let tmp = PathBuf::from(format!("{}.part", target.to_string_lossy()));
    tokio::fs::write(&tmp, &bytes)
        .await
        .map_err(|e| format!("write tmp failed: {e}"))?;
    if tokio::fs::rename(&tmp, &target).await.is_err() {
        tokio::fs::copy(&tmp, &target)
            .await
            .map_err(|e| format!("copy tmp failed: {e}"))?;
        let _ = tokio::fs::remove_file(&tmp).await;
    }

    Ok(())
}

async fn save_local_sync_cache(local_root: &Path, cache: &LocalSyncCache) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(local_root).await?;
    let sync_path = local_root.join("sync_manifest.json");
    let tmp = sync_path.with_extension("json.tmp");
    tokio::fs::write(&tmp, serde_json::to_string_pretty(cache)?).await?;
    let _ = tokio::fs::rename(&tmp, &sync_path).await;

    // Also update local catalog/models.json so offline catalog lookup stays in sync
    let catalog_entries: Vec<_> = cache
        .manifest
        .models
        .iter()
        .map(|m: &ModelAssetDto| {
            let ready = local_root.join(&m.model_file.rel_path).exists();
            m.to_catalog_entry(ready)
        })
        .collect();
    let catalog_dir = local_root.join("catalog");
    tokio::fs::create_dir_all(&catalog_dir).await?;
    let cat_path = catalog_dir.join("models.json");
    let cat_tmp = cat_path.with_extension("json.tmp");
    tokio::fs::write(&cat_tmp, serde_json::to_string_pretty(&catalog_entries)?).await?;
    let _ = tokio::fs::rename(&cat_tmp, &cat_path).await;
    Ok(())
}
