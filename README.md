# 🦙 Interface — Llama Control Panel

Interface graphique Windows pour contrôler [llama.cpp](https://github.com/ggml-org/llama.cpp) : lancer des modèles en mode CLI ou serveur, télécharger les mises à jour CUDA/Vulkan, parcourir HuggingFace et télécharger des GGUF, le tout sans ligne de commande.

## ✨ Fonctionnalités

- **Lancement CLI / Serveur** — Démarrage/arrêt de `llama-cli` et `llama-server` avec configuration visuelle
- **Mise à jour automatique** — Télécharge et installe la dernière version CUDA 13 + Vulkan depuis GitHub avec sauvegarde zip automatique
- **Restauration** — Restaure une version précédente depuis les sauvegardes
- **HuggingFace** — Recherche de modèles GGUF, indice de compatibilité VRAM, téléchargement par fichier avec barre de progression et annulation
- **Conversation** — Affichage en temps réel de la sortie CLI/Serveur avec entrée clavier
- **Paramètres** — Configuration complète des chemins (modèles, exécutables, dossiers CUDA/Vulkan)
- **Aide intégrée** — Onglet Llama Aide pour consulter `llama-cli --help` et `llama-server --help`
- **Français / English** — Sélecteur de langue dans les paramètres
- **Logs** — Filtrage par niveau (Trace/Debug/Info/Warn/Error) et par source, lecture fichier

## 🚀 Utilisation

1. **Télécharge** la dernière version depuis [Releases](https://github.com/ifreetsd-dev/interface_llama_cpp_windows/releases)
2. **Extrais** `interface-v1.0.0-win64.zip` dans un dossier
3. **Configure les chemins** dans l'onglet Paramètres :
   - Dossier des modèles (où sont stockés les `.gguf`)
   - Dossier CUDA (pour les binaires CUDA)
   - Dossier Vulkan (pour les binaires Vulkan)
   - Chemins vers `llama-cli.exe` et `llama-server.exe`
4. **(Optionnel)** Lance la **mise à jour** depuis l'onglet Principal pour télécharger automatiquement les binaires CUDA 13 + Vulkan
5. **Sélectionne un modèle** dans la liste, choisis une configuration, puis lance CLI ou Serveur

## 🛠️ Build

```powershell
# Cloner
git clone https://github.com/ifreetsd-dev/interface_llama_cpp_windows.git
cd interface_llama_cpp_windows

# Compiler (release)
cargo build --release

# L'exécutable se trouve dans target/release/interface.exe
```

### Prérequis

- [Rust](https://rustup.rs/) 1.75+
- [Git](https://git-scm.com/) (optionnel)
- Windows 10/11 (les API NVML et Win32 sont utilisées pour VRAM et infos disque/RAM)

## 📦 Dépendances

| Crate | Utilisation |
|---|---|
| `eframe` / `egui` | Interface graphique |
| `tokio` | Async runtime + processus |
| `reqwest` | Téléchargements HTTP (GitHub, HuggingFace) |
| `zip` | Compression/extraction zip |
| `nvml-wrapper` | Lecture VRAM NVIDIA |
| `serde` / `serde_json` | Configuration persistante |
| `chrono` | Horodatage des logs |
| `rfd` | Sélecteur de fichiers natif |
| `futures` | Streaming de téléchargement |

## 📄 Licence

Ce projet est libre de droit.
