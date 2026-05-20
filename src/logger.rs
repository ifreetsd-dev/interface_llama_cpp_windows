#![allow(dead_code)]

use chrono::Local;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Mutex;

static LOGGER: Mutex<Option<Box<dyn Write + Send>>> = Mutex::new(None);
static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);
static LOG_LEVEL: Mutex<Level> = Mutex::new(Level::Info);
static LOG_ENABLED: Mutex<[bool; 5]> = Mutex::new([true; 5]);

pub fn init() {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    
    let log_dir = exe_dir.join("logs");
    let _ = std::fs::create_dir_all(&log_dir);

    let date = Local::now().format("%Y-%m-%d").to_string();
    let log_path = log_dir.join(format!("interface_{}.log", date));

    let file = match OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&log_path) 
    {
        Ok(f) => Some(Box::new(f) as Box<dyn Write + Send>),
        Err(e) => {
            eprintln!("[LOGGER] ERREUR {}: {}", log_path.display(), e);
            None
        }
    };

    *LOG_PATH.lock().unwrap() = Some(log_path);
    *LOGGER.lock().unwrap() = file;

    log(Level::Info, "Logger", "Démarrage");
}

pub fn set_level(level: u8) {
    if level <= 4 {
        *LOG_LEVEL.lock().unwrap() = Level::from_u8(level);
    }
}

pub fn set_enabled(trace: bool, debug: bool, info: bool, warn: bool, error: bool) {
    *LOG_ENABLED.lock().unwrap() = [trace, debug, info, warn, error];
}

impl Level {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Level::Trace,
            1 => Level::Debug,
            2 => Level::Info,
            3 => Level::Warn,
            _ => Level::Error,
        }
    }
}

pub fn log(level: Level, module: &str, message: &str) {
    let enabled = *LOG_ENABLED.lock().unwrap();
    let idx = level as usize;
    if idx < enabled.len() && !enabled[idx] {
        return;
    }
    
    let min_level = *LOG_LEVEL.lock().unwrap();
    if (level as u8) < (min_level as u8) {
        return;
    }
    
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let line = format!("[{}] [{}] [{}] {}", timestamp, level, module, message);

    eprintln!("{}", line);

    if let Ok(mut guard) = LOGGER.lock() {
        if let Some(ref mut file) = *guard {
            let _ = writeln!(file, "{}", line);
            let _ = file.flush();
        }
    }
}

pub fn log_trace(module: &str, message: &str) {
    log(Level::Trace, module, message);
}

pub fn error(module: &str, err: &impl std::fmt::Display) {
    log(Level::Error, module, &err.to_string());
}

pub fn get_log_path() -> Option<PathBuf> {
    LOG_PATH.lock().ok().and_then(|g| g.as_ref().map(|p| p.clone()))
}

/// Lit les dernières `max_lines` lignes du fichier journal (lecture depuis la fin)
pub fn read_log_content(max_lines: usize) -> Vec<String> {
    let path = match get_log_path() {
        Some(p) => p,
        None => return Vec::new(),
    };
    let mut file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let file_len = match file.seek(SeekFrom::End(0)) {
        Ok(len) => len,
        Err(_) => return Vec::new(),
    };
    if file_len == 0 {
        return Vec::new();
    }
    // Lire au maximum 64 KB depuis la fin
    let read_size = std::cmp::min(file_len, 65536);
    let start = file_len - read_size;
    if file.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut buf = vec![0u8; read_size as usize];
    if file.read_exact(&mut buf).is_err() {
        return Vec::new();
    }
    let content = String::from_utf8_lossy(&buf);
    let lines: Vec<&str> = content.lines().collect();
    let count = lines.len();
    if count <= max_lines {
        lines.iter().map(|l| l.to_string()).collect()
    } else {
        lines[count - max_lines..].iter().map(|l| l.to_string()).collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Level::Trace => write!(f, "TRACE"),
            Level::Debug => write!(f, "DEBUG"),
            Level::Info => write!(f, "INFO"),
            Level::Warn => write!(f, "WARN"),
            Level::Error => write!(f, "ERROR"),
        }
    }
}