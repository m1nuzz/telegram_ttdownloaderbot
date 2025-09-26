use std::path::{PathBuf, Path};
use tokio::fs;
use anyhow::Result;

use crate::yt_dlp_interface::utils::is_executable_present;
use crate::yt_dlp_interface::urls::{get_latest_yt_dlp_url, get_latest_ffmpeg_url};
use crate::yt_dlp_interface::downloader::{download_file, extract_ffmpeg_windows};

pub async fn ensure_binaries(libraries_dir: &Path, output_dir: &Path) -> Result<()> {
    fs::create_dir_all(libraries_dir).await?;
    fs::create_dir_all(output_dir).await?;
    
    let yt_dlp_path = libraries_dir.join(if cfg!(target_os = "windows") { "yt-dlp.exe" } else { "yt-dlp" });
    let ffmpeg_zip_path = libraries_dir.join("ffmpeg-release.zip");
    let ffmpeg_dir_path = libraries_dir.join("ffmpeg");
    let ffmpeg_path = ffmpeg_dir_path.join(if cfg!(target_os = "windows") { "ffmpeg.exe" } else { "ffmpeg" });

    // Check and download/update yt-dlp
    if !is_executable_present(&yt_dlp_path) {
        log::info!("yt-dlp not found, downloading latest version...");
        let yt_dlp_url = get_latest_yt_dlp_url();
        download_file(&yt_dlp_url, &yt_dlp_path).await?;
        
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&yt_dlp_path).await?.permissions();
            perms.set_mode(0o755);  // Make executable
            fs::set_permissions(&yt_dlp_path, perms).await?;
        }
    } else {
        log::info!("yt-dlp already exists at {:?}", yt_dlp_path);
    }

    // Check and download/update ffmpeg
    if !is_executable_present(&ffmpeg_path) {
        log::info!("ffmpeg not found, downloading latest version...");
        
        if cfg!(target_os = "windows") {
            // Download the zip file for Windows
            let ffmpeg_url = get_latest_ffmpeg_url();
            download_file(&ffmpeg_url, &ffmpeg_zip_path).await?;
            
            // Extract ffmpeg.exe from the zip file
            fs::create_dir_all(&ffmpeg_dir_path).await?;
            
            extract_ffmpeg_windows(&ffmpeg_zip_path, &ffmpeg_dir_path).await?;
            
            // After extraction, verify that ffmpeg.exe exists in the expected location
            if !is_executable_present(&ffmpeg_path) {
                log::error!("ffmpeg.exe was not found in the expected location after extraction: {:?}", ffmpeg_path);
                log::info!("Contents of ffmpeg directory: {:?}", 
                    {
                        let mut entries = fs::read_dir(&ffmpeg_dir_path).await?;
                        let mut paths = Vec::new();
                        while let Some(entry) = entries.next_entry().await? {
                            paths.push(entry.path());
                        }
                        paths
                    }
                );
                
                // Try to find ffmpeg.exe in the extracted directory structure
                let extracted_ffmpeg = find_ffmpeg_in_extracted_dir(&ffmpeg_dir_path).await;
                if let Some(found_path) = extracted_ffmpeg {
                    log::info!("Found ffmpeg at {:?}, copying to expected location", found_path);
                    fs::create_dir_all(ffmpeg_path.parent().unwrap()).await?;
                    fs::copy(&found_path, &ffmpeg_path).await?;
                }
            }
        } else {
            // For non-Windows (Linux/Android/MacOS), we might need a different approach
            // For now, just download the appropriate binary
            log::info!("Downloading ffmpeg for non-Windows platform...");
            let ffmpeg_url = get_latest_ffmpeg_url();
            
            // Create directory for ffmpeg
            fs::create_dir_all(ffmpeg_path.parent().unwrap()).await?;
            
            // For now, just download the tar.xz file and we'll assume it contains ffmpeg
            // In practice, you might need to handle different extraction based on the archive type
            download_file(&ffmpeg_url, &ffmpeg_path.with_extension("tar.xz")).await?;
            
            // For Termux on Android, ffmpeg might need to be installed differently
            if cfg!(target_os = "linux") {
                log::info!("For Linux/Android systems, you might need to install ffmpeg manually or use package manager");
                log::info!("You can install ffmpeg with: apt install ffmpeg (in Termux) or equivalent package manager");
            }
        }
    } else {
        log::info!("ffmpeg already exists at {:?}", ffmpeg_path);
    }

    Ok(())
}

// Helper function to find ffmpeg.exe in the extracted directory structure
pub async fn find_ffmpeg_in_extracted_dir(base_dir: &PathBuf) -> Option<PathBuf> {
    let mut stack = vec![base_dir.clone()];
    
    while let Some(current_dir) = stack.pop() {
        if let Ok(mut entries) = tokio::fs::read_dir(&current_dir).await {
            while let Some(entry) = entries.next_entry().await.transpose() {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    
                    if path.is_file() && 
                       path.file_name().map_or(false, |name| name == "ffmpeg.exe") {
                        return Some(path);
                    } else if path.is_dir() {
                        stack.push(path);
                    }
                }
            }
        }
    }
    
    None
}