use std::sync::Mutex;
use std::time::{Duration, Instant};

static DISK_CACHE: Mutex<Option<(Vec<DiskInfo>, Instant)>> = Mutex::new(None);
const DISK_CACHE_DURATION: Duration = Duration::from_secs(5);

#[derive(Clone, Debug)]
pub struct DiskInfo {
    pub drive: String,
    pub total_gb: f64,
    pub free_gb: f64,
}

#[derive(Clone, Debug, Default)]
pub struct RamInfo {
    pub total_mb: u64,
    pub used_mb: u64,
    pub free_mb: u64,
}

#[cfg(target_os = "windows")]
fn query_disks() -> Vec<DiskInfo> {
    let mut drives = Vec::new();
    unsafe {
        let bitmask = GetLogicalDrives();
        for i in 0..26 {
            if bitmask & (1 << i) != 0 {
                let letter = (b'A' + i as u8) as char;
                let root = format!("{}:\\", letter);
                let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();

                let mut free_avail: u64 = 0;
                let mut total: u64 = 0;
                let mut _total_free: u64 = 0;

                if GetDiskFreeSpaceExW(wide.as_ptr(), &mut free_avail, &mut total, &mut _total_free) != 0
                    && total > 0
                {
                    drives.push(DiskInfo {
                        drive: format!("{}:", letter),
                        total_gb: total as f64 / 1_073_741_824.0,
                        free_gb: free_avail as f64 / 1_073_741_824.0,
                    });
                }
            }
        }
    }
    drives
}

#[cfg(not(target_os = "windows"))]
fn query_disks() -> Vec<DiskInfo> {
    Vec::new()
}

#[cfg(target_os = "windows")]
extern "system" {
    fn GetLogicalDrives() -> u32;
    fn GetDiskFreeSpaceExW(
        lpDirectoryName: *const u16,
        lpFreeBytesAvailable: *mut u64,
        lpTotalNumberOfBytes: *mut u64,
        lpTotalNumberOfFreeBytes: *mut u64,
    ) -> i32;
    fn GlobalMemoryStatusEx(lpBuffer: *mut MEMORYSTATUSEX) -> i32;
}

#[repr(C)]
#[allow(non_snake_case)]
struct MEMORYSTATUSEX {
    dwLength: u32,
    dwMemoryLoad: u32,
    ullTotalPhys: u64,
    ullAvailPhys: u64,
    ullTotalPageFile: u64,
    ullAvailPageFile: u64,
    ullTotalVirtual: u64,
    ullAvailVirtual: u64,
    ullAvailExtendedVirtual: u64,
}

pub fn get_ram_info() -> RamInfo {
    #[cfg(target_os = "windows")]
    {
        unsafe {
            let mut mem = MEMORYSTATUSEX {
                dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
                dwMemoryLoad: 0,
                ullTotalPhys: 0,
                ullAvailPhys: 0,
                ullTotalPageFile: 0,
                ullAvailPageFile: 0,
                ullTotalVirtual: 0,
                ullAvailVirtual: 0,
                ullAvailExtendedVirtual: 0,
            };
            if GlobalMemoryStatusEx(&mut mem) != 0 {
                let total_mb = (mem.ullTotalPhys / (1024 * 1024)) as u64;
                let free_mb = (mem.ullAvailPhys / (1024 * 1024)) as u64;
                return RamInfo {
                    total_mb,
                    used_mb: total_mb.saturating_sub(free_mb),
                    free_mb,
                };
            }
        }
    }
    RamInfo::default()
}

pub fn get_disk_info() -> Vec<DiskInfo> {
    let mut cache = DISK_CACHE.lock().unwrap();
    if let Some((info, time)) = cache.as_ref() {
        if time.elapsed() < DISK_CACHE_DURATION {
            return info.clone();
        }
    }
    let info = query_disks();
    *cache = Some((info.clone(), Instant::now()));
    info
}

pub fn format_gb(gb: f64) -> String {
    if gb >= 1000.0 {
        format!("{:.2} To", gb / 1024.0)
    } else {
        format!("{:.2} Go", gb)
    }
}

pub fn check_free_space(path: &std::path::Path, needed_bytes: u64) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let path_str = path.to_string_lossy();
        let root = if path_str.len() >= 2 && path_str.as_bytes()[1] == b':' {
            format!("{}\\", &path_str[..2])
        } else {
            return Ok(());
        };
        let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
        let mut free_avail: u64 = 0;
        let mut _total: u64 = 0;
        let mut _total_free: u64 = 0;
        unsafe {
            if GetDiskFreeSpaceExW(wide.as_ptr(), &mut free_avail, &mut _total, &mut _total_free) == 0 {
                return Ok(());
            }
        }
        if free_avail < needed_bytes {
            let free_gb = free_avail as f64 / 1_073_741_824.0;
            let need_gb = needed_bytes as f64 / 1_073_741_824.0;
            return Err(format!(
                "Espace insuffisant sur {} : {:.2} Go libre, besoin de {:.2} Go",
                &path_str[..2], free_gb, need_gb
            ));
        }
    }
    Ok(())
}
