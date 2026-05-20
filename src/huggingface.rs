use serde::Deserialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default, Deserialize)]
pub struct HfModelInfo {
    pub id: String,
    pub downloads: Option<i64>,
    #[serde(default)]
    pub siblings: Vec<HfSibling>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct HfSibling {
    pub rfilename: String,
    #[serde(default)]
    pub size: Option<i64>,
}

pub async fn search_models(query: &str) -> Result<Vec<HfModelInfo>, String> {
    let client = reqwest::Client::builder()
        .user_agent("llama-interface/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let url = reqwest::Url::parse_with_params(
        "https://huggingface.co/api/models",
        &[
            ("search", query),
            ("sort", "downloads"),
            ("direction", "-1"),
            ("limit", "50"),
        ],
    )
    .map_err(|e| e.to_string())?;

    let resp = client.get(url).send().await.map_err(|e| format!("Erreur requête: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let models: Vec<HfModelInfo> = resp.json().await.map_err(|e| format!("Erreur parsing: {}", e))?;
    Ok(models)
}

/// Récupère les détails d'un modèle (fichiers + tailles) via l'API complète
pub async fn get_model_details(model_id: &str) -> Result<HfModelInfo, String> {
    let client = reqwest::Client::builder()
        .user_agent("llama-interface/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("https://huggingface.co/api/models/{}", model_id);
    let resp = client.get(&url).send().await.map_err(|e| format!("Erreur requête: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let model: HfModelInfo = resp.json().await.map_err(|e| format!("Erreur parsing: {}", e))?;
    Ok(model)
}

pub async fn download_specific_gguf(
    model_id: &str,
    filename: &str,
    dest_dir: &Path,
    temp_dir: &Path,
    cancel: Option<Arc<AtomicBool>>,
    progress: Option<Arc<Mutex<Option<(String, u64, u64)>>>>,
) -> Result<String, String> {
    let file_url = format!("https://huggingface.co/{}/resolve/main/{}", model_id, filename);
    let file_name = filename.rsplit('/').next().unwrap_or(filename);

    let _ = std::fs::create_dir_all(temp_dir);
    let temp_path = temp_dir.join(format!("{}.part", file_name));

    crate::downloader::download_file(&file_url, &temp_path, progress.clone(), cancel.clone()).await?;

    if let Some(ref c) = cancel {
        if c.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(&temp_path);
            return Err("Téléchargement annulé".into());
        }
    }

    let final_path = dest_dir.join(file_name);
    std::fs::rename(&temp_path, &final_path).map_err(|e| format!("Erreur déplacement fichier: {}", e))?;

    Ok(file_name.to_string())
}

pub async fn download_model_gguf(
    model_id: &str,
    dest_dir: &Path,
    temp_dir: &Path,
    cancel: Option<Arc<AtomicBool>>,
    progress: Option<Arc<Mutex<Option<(String, u64, u64)>>>>,
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .user_agent("llama-interface/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    // Fetch siblings to find GGUF files
    let url = format!("https://huggingface.co/api/models/{}", model_id);
    let resp = client.get(&url).send().await.map_err(|e| format!("Erreur requête: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let model: HfModelInfo = resp.json().await.map_err(|e| format!("Erreur parsing: {}", e))?;
    
    let gguf_files: Vec<&HfSibling> = model.siblings.iter()
        .filter(|s| s.rfilename.ends_with(".gguf"))
        .collect();

    if gguf_files.is_empty() {
        return Err("Aucun fichier GGUF trouvé".into());
    }

    // Download the largest GGUF file
    let target = gguf_files.iter()
        .max_by_key(|s| s.size.unwrap_or(0))
        .ok_or("Aucun fichier GGUF")?;

    let file_url = format!("https://huggingface.co/{}/resolve/main/{}", model_id, target.rfilename);
    let file_name = target.rfilename.rsplit('/').next().unwrap_or(&target.rfilename);

    // Télécharger d'abord dans le dossier temporaire
    let _ = std::fs::create_dir_all(temp_dir);
    let temp_path = temp_dir.join(format!("{}.part", file_name));

    let cancel_file = cancel.clone();
    crate::downloader::download_file(&file_url, &temp_path, progress.clone(), cancel_file).await?;

    // Vérifier annulation entre les étapes
    if let Some(ref c) = cancel {
        if c.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(&temp_path);
            return Err("Téléchargement annulé".into());
        }
    }

    // Déplacer vers le dossier de destination
    let final_path = dest_dir.join(file_name);
    std::fs::rename(&temp_path, &final_path).map_err(|e| format!("Erreur déplacement fichier: {}", e))?;

    Ok(file_name.to_string())
}
