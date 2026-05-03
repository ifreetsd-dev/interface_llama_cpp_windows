#![allow(dead_code)]

use crate::config::{GlobalPaths, LlamaConfig, RunMode};
use crate::error::{Error, Result};
use crate::logger;
use crossbeam_channel::Sender;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::mpsc;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

pub struct ProcessManager {
    cli_child: Mutex<Option<Child>>,
    server_child: Mutex<Option<Child>>,
    cli_prompt_tx: Arc<AsyncMutex<Option<mpsc::Sender<String>>>>,
    pub log_tx: Sender<String>,
}

impl ProcessManager {
    pub fn new(log_tx: Sender<String>) -> Self {
        Self {
            cli_child: Mutex::new(None),
            server_child: Mutex::new(None),
            cli_prompt_tx: Arc::new(AsyncMutex::new(None)),
            log_tx,
        }
    }

    pub fn is_cli_running(&self) -> bool { 
        if let Some(child) = self.cli_child.lock().unwrap().as_mut() {
            match child.try_wait() {
                Ok(None) => true,
                _ => false,
            }
        } else {
            false
        }
    }
    
    pub fn is_server_running(&self) -> bool { 
        if let Some(child) = self.server_child.lock().unwrap().as_mut() {
            match child.try_wait() {
                Ok(None) => true,
                _ => false,
            }
        } else {
            false
        }
    }

    fn kill_process_by_exe(exe_name: &str) {
        #[cfg(windows)]
        {
            use std::process::Command as StdCommand;
            let mut cmd = StdCommand::new("cmd");
            cmd.args(&["/C", "taskkill", "/F", "/IM", exe_name]);
            cmd.creation_flags(0x08000000);
            let _ = cmd.spawn();
        }
        #[cfg(not(windows))]
        {
            use std::process::Command as StdCommand;
            let _ = StdCommand::new("pkill")
                .arg("-f")
                .arg(exe_name)
                .spawn();
        }
    }

    pub async fn start(&self, cfg: &LlamaConfig, paths: &GlobalPaths) -> Result<()> {
        // Clone pour éviter le move
        let mode = cfg.mode;
        let use_vulkan = cfg.use_vulkan;
        
        logger::log_trace("process", &format!("Starting mode={:?} vulkan={}", mode, use_vulkan));
        
        let exe = match (mode, use_vulkan) {
            (RunMode::Cli, true) => &paths.cli_vulkan,
            (RunMode::Cli, false) => &paths.cli_cuda,
            (RunMode::Server, true) => &paths.server_vulkan,
            (RunMode::Server, false) => &paths.server_cuda,
        };
        
        logger::log_trace("process", &format!("Executable: {}", exe));
        
        if exe.is_empty() { 
            logger::log(logger::Level::Error, "process", "Executable path is empty");
            return Err(Error::process("Chemin exécutable vide pour ce mode/backend")); 
        }

let mut cmd = Command::new(exe);
        cmd.arg("-m").arg(&cfg.model_path);

        if cfg.ngl >= 0 {
            cmd.arg("-ngl").arg(cfg.ngl.to_string());
        } else {
            cmd.arg("-ngl").arg("all");
        }

        cmd.arg("-c").arg(cfg.ctx_size.to_string())
           .arg("-t").arg(cfg.threads.to_string())
           .arg("--temp").arg(cfg.temperature.to_string())
           .arg("-n").arg(cfg.max_tokens.to_string());

        if !cfg.additional_args.trim().is_empty() {
            let all_args = cfg.additional_args.replace("\r\n", "\n").replace("\r", "\n");
            for line in all_args.lines() {
                let arg = line.trim();
                if !arg.is_empty() {
                    if arg.contains(' ') {
                        for part in arg.split_whitespace() {
                            cmd.arg(part);
                        }
                    } else {
                        cmd.arg(arg);
                    }
                }
            }
        }

        if cfg.use_vulkan {
            cmd.env("GGML_VULKAN", "1");
        }
        if cfg.mode == RunMode::Server {
            cmd.arg("--host").arg(&cfg.server_host)
               .arg("--port").arg(cfg.server_port.to_string())
               .arg("-np").arg(cfg.server_parallel.to_string());
            logger::log(logger::Level::Info, "process", &format!("[SRV] Listening on {}:{}", cfg.server_host, cfg.server_port));
        } else {
            cmd.arg("-cnv");
        }

        cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());

        #[cfg(windows)]
        {
            cmd.creation_flags(0x08000000);
        }

        let log_tx = self.log_tx.clone();
        let extra_display = if !cfg.additional_args.trim().is_empty() {
            let all_args = cfg.additional_args.replace("\r\n", "\n").replace("\r", "\n");
            all_args.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ")
        } else { String::new() };
        let cmd_display = if extra_display.is_empty() {
            format!("{} -m {} -ngl {} -c {} -t {} --temp {} -n {}", exe, cfg.model_path,
                if cfg.ngl >= 0 { cfg.ngl.to_string() } else { "all".to_string() },
                cfg.ctx_size, cfg.threads, cfg.temperature, cfg.max_tokens)
        } else {
            format!("{} -m {} -ngl {} -c {} -t {} --temp {} -n {} {}", exe, cfg.model_path,
                if cfg.ngl >= 0 { cfg.ngl.to_string() } else { "all".to_string() },
                cfg.ctx_size, cfg.threads, cfg.temperature, cfg.max_tokens, extra_display)
        };
        let _ = log_tx.send(format!("[CLI] Cmd: {}", cmd_display));

let prefix = "[CLI]";
        if cfg.mode == RunMode::Cli {
            cmd.stdin(std::process::Stdio::piped());
            let mut child = cmd.spawn()?;
            let stdin = child.stdin.take().unwrap();
            let (tx, mut rx) = mpsc::channel::<String>(100);
            *self.cli_prompt_tx.lock().await = Some(tx);

            let out = child.stdout.take().unwrap();
            let err = child.stderr.take().unwrap();

            tokio::spawn(read_stream(out, log_tx.clone(), prefix));
            tokio::spawn(read_stream(err, log_tx, prefix));
            
            tokio::spawn(async move {
                let mut writer = tokio::io::BufWriter::new(stdin);
                while let Some(line) = rx.recv().await {
                    let _ = writer.write_all(format!("{}\n", line).as_bytes()).await;
                    let _ = writer.flush().await;
                }
            });
            *self.cli_child.lock().unwrap() = Some(child);
        } else {
            let prefix = "[SRV]";
            let cmd_display = format!("{} --model {} -ngl {} -c {} -t {} --temp {} -n {} --host {} --port {} -np {}",
                exe, cfg.model_path,
                if cfg.ngl >= 0 { cfg.ngl.to_string() } else { "all".to_string() },
                cfg.ctx_size, cfg.threads, cfg.temperature, cfg.max_tokens,
                cfg.server_host, cfg.server_port, cfg.server_parallel);
            let _ = log_tx.send(format!("[SRV] Cmd: {}", cmd_display));
            let mut child = cmd.spawn()?;
            let out = child.stdout.take().unwrap();
            let err = child.stderr.take().unwrap();
            tokio::spawn(read_stream(out, log_tx.clone(), prefix));
            tokio::spawn(read_stream(err, log_tx, prefix));
            *self.server_child.lock().unwrap() = Some(child);
        }
        Ok(())
    }

pub fn stop_cli(&self) -> Result<()> {
        let child_opt = self.cli_child.lock().unwrap().take();
        if let Some(mut child) = child_opt { 
            match child.try_wait() {
                Ok(Some(status)) => {
                    let _ = self.log_tx.send(format!("CLI exited: {:?}", status));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = self.log_tx.send("CLI killed".into());
                }
                Err(e) => {
                    let _ = self.log_tx.send(format!("Erreur stop CLI: {}", e));
                }
            }
        }
        
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async { *self.cli_prompt_tx.lock().await = None });
        let _ = self.log_tx.send("⏹️ CLI arrêté".into());
        Ok(())
    }
    
    pub fn stop_server(&self) -> Result<()> {
        let child_opt = self.server_child.lock().unwrap().take();
        if let Some(mut child) = child_opt { 
            match child.try_wait() {
                Ok(Some(status)) => {
                    let _ = self.log_tx.send(format!("Serveur exited: {:?}", status));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = self.log_tx.send("Serveur killed".into());
                }
                Err(e) => {
                    let _ = self.log_tx.send(format!("Erreur stop serveur: {}", e));
                }
            }
        }
        
        let _ = self.log_tx.send("⏹️ Serveur arrêté".into());
        Ok(())
    }

    pub async fn stop_all(&self, paths: &GlobalPaths) {
        Self::kill_all_by_paths(paths);
        
        if let Some(mut child) = self.cli_child.lock().unwrap().take() {
            let _ = child.kill();
        }
        
        if let Some(mut child) = self.server_child.lock().unwrap().take() {
            let _ = child.kill();
        }
        
        if let Ok(mut guard) = self.cli_prompt_tx.try_lock() {
            *guard = None;
        }
        
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        
        Self::kill_all_by_paths(paths);
        
        let _ = self.log_tx.send("⏹️ Processus arrêté".to_string());
    }

    pub fn kill_all_by_paths(paths: &GlobalPaths) {
        let executables = [
            paths.cli_cuda.as_str(),
            paths.cli_vulkan.as_str(),
            paths.server_cuda.as_str(),
            paths.server_vulkan.as_str(),
        ];
        
        for exe in executables.iter() {
            if !exe.is_empty() {
                if let Some(name) = exe.rsplit(['\\', '/']).next() {
                    if !name.is_empty() {
                        Self::kill_process_by_exe(name);
                    }
                }
            }
        }
    }

    pub fn send_cli_prompt(&self, prompt: &str) {
        // Note: on ne peut pas .await ici, donc on ignore si le channel est plein
        if let Ok(guard) = self.cli_prompt_tx.try_lock() {
            if let Some(tx) = guard.as_ref() { let _ = tx.try_send(prompt.to_string()); }
        }
    }
}

async fn read_stream(
    stream: impl tokio::io::AsyncRead + Unpin, 
    tx: Sender<String>, 
    prefix: &str
) {
    let mut reader = BufReader::new(stream).lines();
    while let Ok(Some(line)) = reader.next_line().await {
        let _ = tx.send(format!("{} {}", prefix, line));
    }
}