use serde::Deserialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use tokio::io::AsyncWriteExt;

#[derive(Clone)]
pub struct ReleaseInfo {
    pub tag: String,
    pub cuda_url: String,
    pub cuda_dll_url: String,
    pub vulkan_url: String,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

pub async fn check_latest() -> Result<ReleaseInfo, String> {
    crate::logger::log(crate::logger::Level::Info, "download", "Vérification de la dernière version...");

    let client = reqwest::Client::builder()
        .user_agent("llama-interface/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get("https://api.github.com/repos/ggml-org/llama.cpp/releases/latest")
        .send()
        .await
        .map_err(|e| format!("Erreur requête: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let release: GitHubRelease = resp
        .json()
        .await
        .map_err(|e| format!("Erreur parsing: {}", e))?;

    let tag = release.tag_name.clone();
    crate::logger::log(crate::logger::Level::Info, "download", &format!("Version trouvée: {}", tag));

    // Log tous les assets disponibles
    for asset in &release.assets {
        crate::logger::log(crate::logger::Level::Debug, "download", &format!("Asset: {}", asset.name));
    }

    let mut cuda_url = String::new();
    let mut cuda_dll_url = String::new();
    let mut vulkan_url = String::new();

    for asset in &release.assets {
        let name = asset.name.to_lowercase();
        // CUDA 13.1 binary: llama-bXXXX-bin-win-cuda-13.1-x64.zip
        // IMPORTANT: exclure "cudart" car cudart-llama-... contient aussi "win-cuda-13.1" et "llama"
        if name.contains("win-cuda-13.1") && name.contains("llama") && !name.contains("cudart") && name.ends_with(".zip") {
            if cuda_url.is_empty() {
                cuda_url = asset.browser_download_url.clone();
                crate::logger::log(crate::logger::Level::Info, "download", &format!("CUDA binaire trouvé: {}", asset.name));
            }
        }
        // CUDA 13.1 DLLs: cudart-llama-bin-win-cuda-13.1-x64.zip
        if name.contains("cudart") && name.contains("win-cuda-13.1") && name.ends_with(".zip") {
            if cuda_dll_url.is_empty() {
                cuda_dll_url = asset.browser_download_url.clone();
                crate::logger::log(crate::logger::Level::Info, "download", &format!("CUDA DLLs trouvé: {}", asset.name));
            }
        }
        // Windows Vulkan: llama-bXXXX-bin-win-vulkan-x64.zip
        if name.contains("win-vulkan") && name.contains("llama") && name.ends_with(".zip") {
            if vulkan_url.is_empty() {
                vulkan_url = asset.browser_download_url.clone();
                crate::logger::log(crate::logger::Level::Info, "download", &format!("Vulkan trouvé: {}", asset.name));
            }
        }
    }

    if cuda_url.is_empty() {
        crate::logger::log(crate::logger::Level::Warn, "download", "CUDA binaire non trouvé au premier passage, recherche élargie...");
        for asset in &release.assets {
            let name = asset.name.to_lowercase();
            if name.contains("cuda") && name.ends_with(".zip") && name.contains("llama") && !name.contains("cudart") {
                cuda_url = asset.browser_download_url.clone();
                crate::logger::log(crate::logger::Level::Info, "download", &format!("CUDA binaire (fallback): {}", asset.name));
                break;
            }
        }
    }
    if cuda_dll_url.is_empty() {
        crate::logger::log(crate::logger::Level::Warn, "download", "CUDA DLLs non trouvé au premier passage, recherche élargie...");
        for asset in &release.assets {
            let name = asset.name.to_lowercase();
            if name.contains("cudart") && name.ends_with(".zip") {
                cuda_dll_url = asset.browser_download_url.clone();
                crate::logger::log(crate::logger::Level::Info, "download", &format!("CUDA DLLs (fallback): {}", asset.name));
                break;
            }
        }
    }
    if vulkan_url.is_empty() {
        crate::logger::log(crate::logger::Level::Warn, "download", "Vulkan non trouvé au premier passage, recherche élargie...");
        for asset in &release.assets {
            let name = asset.name.to_lowercase();
            if name.contains("win-vulkan") && name.ends_with(".zip") {
                vulkan_url = asset.browser_download_url.clone();
                crate::logger::log(crate::logger::Level::Info, "download", &format!("Vulkan (fallback): {}", asset.name));
                break;
            }
        }
    }

    if cuda_url.is_empty() {
        crate::logger::log(crate::logger::Level::Error, "download", "Aucun asset CUDA trouvé !");
    }
    if cuda_dll_url.is_empty() {
        crate::logger::log(crate::logger::Level::Warn, "download", "Aucun asset CUDA DLLs trouvé");
    }
    if vulkan_url.is_empty() {
        crate::logger::log(crate::logger::Level::Error, "download", "Aucun asset Vulkan trouvé !");
    }

    Ok(ReleaseInfo {
        tag,
        cuda_url,
        cuda_dll_url,
        vulkan_url,
    })
}

pub async fn download_file(url: &str, dest: &Path, progress: Option<Arc<Mutex<Option<(String, u64, u64)>>>>, cancel: Option<Arc<AtomicBool>>) -> Result<(), String> {
    let file_name = url.rsplit('/').next().unwrap_or("file.zip").to_string();
    crate::logger::log(crate::logger::Level::Info, "download", &format!("Début téléchargement: {} → {}", file_name, dest.display()));

    let client = reqwest::Client::builder()
        .user_agent("llama-interface/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Erreur téléchargement {}: {}", file_name, e))?;

    let status = resp.status();
    crate::logger::log(crate::logger::Level::Debug, "download", &format!("Réponse HTTP {} pour {}", status, file_name));
    if !status.is_success() {
        return Err(format!("HTTP {} pour {}", status, file_name));
    }

    let total = resp.content_length().unwrap_or(0);
    crate::logger::log(crate::logger::Level::Info, "download", &format!("Taille fichier {}: {} octets", file_name, total));

    // Vérifier l'espace disque avant de télécharger
    if let Some(parent) = dest.parent() {
        if total > 0 {
            crate::disk::check_free_space(parent, total)?;
        }
    }

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
    }

    let mut file = tokio::fs::File::create(dest).await.map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();

    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        // Vérifier l'annulation
        if let Some(ref c) = cancel {
            if c.load(Ordering::Relaxed) {
                crate::logger::log(crate::logger::Level::Warn, "download", "Téléchargement annulé par l'utilisateur");
                std::mem::drop(file);
                let _ = tokio::fs::remove_file(dest).await;
                return Err("Téléchargement annulé".into());
            }
        }
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if total > 0 && downloaded % (1024 * 1024) < chunk.len() as u64 {
            crate::logger::log(crate::logger::Level::Debug, "download", &format!("{}: {}/{} Mo", file_name, downloaded / (1024*1024), total / (1024*1024)));
        }
        if let Some(ref p) = progress {
            if let Ok(mut guard) = p.try_lock() {
                *guard = Some((file_name.clone(), downloaded, total));
            }
        }
    }

    file.flush().await.map_err(|e| e.to_string())?;
    crate::logger::log(crate::logger::Level::Info, "download", &format!("Téléchargement terminé: {} ({} octets)", file_name, downloaded));
    Ok(())
}

pub fn zip_dir(src: &Path, dst: &Path) -> Result<(), String> {
    let file = std::fs::File::create(dst).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    fn add_dir(w: &mut zip::ZipWriter<std::fs::File>, base: &Path, dir: &Path, opts: zip::write::SimpleFileOptions) -> Result<(), String> {
        for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let name = path.strip_prefix(base).map_err(|e| e.to_string())?;
            if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                w.add_directory(name.to_string_lossy(), opts).map_err(|e| e.to_string())?;
                add_dir(w, base, &path, opts)?;
            } else {
                w.start_file(name.to_string_lossy(), opts).map_err(|e| e.to_string())?;
                let mut f = std::fs::File::open(&path).map_err(|e| e.to_string())?;
                std::io::copy(&mut f, w).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    add_dir(&mut zip, src, src, options)?;
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn extract_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let buf = tokio::fs::read(zip_path).await.map_err(|e| e.to_string())?;
    let dest = dest.to_path_buf();

    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(buf)).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
            let outpath = dest.join(entry.name());

            if entry.name().ends_with('/') {
                std::fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
            } else {
                if let Some(parent) = outpath.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let mut outfile = std::fs::File::create(&outpath).map_err(|e| e.to_string())?;
                std::io::copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
            }
        }

        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
