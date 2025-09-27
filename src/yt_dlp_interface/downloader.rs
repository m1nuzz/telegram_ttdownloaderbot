use std::path::PathBuf;
use tokio::fs;
use tokio::io;
use zip::ZipArchive;
use anyhow::Result;
use std::io::Read;

#[cfg(unix)]
use tar::Archive;

#[cfg(unix)]
use xz2::read::XzDecoder;

#[cfg(target_os = "macos")]
use sevenz_rust::decompress_file as decompress_7z;

pub async fn download_file(url: &str, path: &PathBuf) -> Result<()> {
    log::info!("Downloading from {} to {:?}", url, path);
    
    let client = reqwest::Client::new();
    let mut response = client.get(url).send().await.map_err(|e| {
        log::error!("Failed to send GET request to {}: {:?}", url, e);
        anyhow::anyhow!("Failed to send GET request to {}: {:?}", url, e)
    })?;
    
    if !response.status().is_success() {
        log::error!("Download failed for {}: HTTP status {}", url, response.status());
        return Err(anyhow::anyhow!("Download failed for {}: HTTP status {}", url, response.status()));
    }

    let mut file = fs::File::create(path).await.map_err(|e| {
        log::error!("Failed to create file {:?}: {:?}", path, e);
        anyhow::anyhow!("Failed to create file {:?}: {:?}", path, e)
    })?;
    
    // Read the response body in chunks and write to the file
    while let Some(chunk) = response.chunk().await.map_err(|e| {
        log::error!("Failed to read chunk from response for {}: {:?}", url, e);
        anyhow::anyhow!("Failed to read chunk from response for {}: {:?}", url, e)
    })? {
        io::copy(&mut chunk.as_ref(), &mut file).await.map_err(|e| {
            log::error!("Failed to write chunk to file {:?}: {:?}", path, e);
            anyhow::anyhow!("Failed to write chunk to file {:?}: {:?}", path, e)
        })?;
    }
    
    log::info!("Download completed successfully to {:?}", path);
    Ok(())
}

pub async fn extract_ffmpeg_windows(zip_path: &PathBuf, extract_to: &PathBuf) -> Result<()> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    
    let mut ffmpeg_extracted = false;
    let mut ffprobe_extracted = false;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let file_name = PathBuf::from(file.name());

        if file_name.ends_with("ffmpeg.exe") {
            let outpath = extract_to.join("ffmpeg.exe");
            
            let mut outfile = fs::File::create(&outpath).await?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            io::copy(&mut buffer.as_slice(), &mut outfile).await?;
            
            log::info!("Extracted ffmpeg.exe to {:?}", outpath);
            ffmpeg_extracted = true;
        } else if file_name.ends_with("ffprobe.exe") {
            let outpath = extract_to.join("ffprobe.exe");
            
            let mut outfile = fs::File::create(&outpath).await?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            io::copy(&mut buffer.as_slice(), &mut outfile).await?;
            
            log::info!("Extracted ffprobe.exe to {:?}", outpath);
            ffprobe_extracted = true;
        }

        if ffmpeg_extracted && ffprobe_extracted {
            break; // Both found, no need to continue
        }
    }
    
    Ok(())
}

#[cfg(target_os = "macos")]
pub async fn extract_ffmpeg_macos(archive_path: &PathBuf, extract_to: &PathBuf) -> Result<()> {
    use std::path::Path;
    use tokio::fs;
    
    // Create the extraction directory
    fs::create_dir_all(extract_to).await?;
    
    // Extract the 7z archive
    let archive_result = decompress_7z(
        archive_path.as_path(),
        extract_to.as_path(),
        |entry, _size| {
            if entry.name.ends_with("ffmpeg") || entry.name.ends_with("ffprobe") {
                Ok(true) // We want to extract this file
            } else {
                Ok(false) // Skip other files
            }
        }
    );
    
    if archive_result.is_err() {
        return Err(anyhow::anyhow!("Failed to extract 7z archive: {:?}", archive_result.err()));
    }
    
    // Find the extracted binaries and rename if necessary
    // Look for the extracted files in the extraction directory
    let mut ffmpeg_found = false;
    let mut ffprobe_found = false;
    
    if let Ok(entries) = std::fs::read_dir(extract_to) {
        for entry in entries {
            if let Ok(entry) = entry {
                let file_name = entry.file_name();
                
                if file_name.to_string_lossy().to_lowercase().contains("ffmpeg") 
                    && !file_name.to_string_lossy().to_lowercase().contains("ffprobe") {
                    
                    let ffmpeg_path = extract_to.join("ffmpeg");
                    if ffmpeg_path != entry.path() {
                        std::fs::rename(entry.path(), &ffmpeg_path)?;
                    }
                    
                    // Set executable permissions
                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = std::fs::metadata(&ffmpeg_path)?.permissions();
                    perms.set_mode(0o755);
                    
                    log::info!("Extracted ffmpeg to {:?}", ffmpeg_path);
                    ffmpeg_found = true;
                } else if file_name.to_string_lossy().to_lowercase().contains("ffprobe") {
                    let ffprobe_path = extract_to.join("ffprobe");
                    if ffprobe_path != entry.path() {
                        std::fs::rename(entry.path(), &ffprobe_path)?;
                    }
                    
                    // Set executable permissions
                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = std::fs::metadata(&ffprobe_path)?.permissions();
                    perms.set_mode(0o755);
                    
                    log::info!("Extracted ffprobe to {:?}", ffprobe_path);
                    ffprobe_found = true;
                }
                
                if ffmpeg_found && ffprobe_found {
                    break;
                }
            }
        }
    }
    
    if !ffmpeg_found {
        return Err(anyhow::anyhow!("ffmpeg binary not found in 7z archive"));
    }
    if !ffprobe_found {
        return Err(anyhow::anyhow!("ffprobe binary not found in 7z archive"));
    }
    
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
pub async fn extract_ffmpeg_unix(archive_path: &PathBuf, extract_to: &PathBuf) -> Result<()> {
    use std::path::Path;
    use tokio::fs;
    use std::fs::File;

    // Create the extraction directory
    fs::create_dir_all(extract_to).await?;
    
    // Open the archive file
    let file = File::open(archive_path)?;
    let decompressed = XzDecoder::new(file);
    let mut archive = Archive::new(decompressed);
    
    let mut ffmpeg_extracted = false;
    let mut ffprobe_extracted = false;
    
    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?;
        
        // Check if this is the ffmpeg or ffprobe binary
        if entry_path.file_name().map_or(false, |name| name == "ffmpeg") {
            let output_path = extract_to.join("ffmpeg");
            
            // Extract the file
            let mut outfile = std::fs::File::create(&output_path)?;
            std::io::copy(&mut entry, &mut outfile)?;
            
            // Set executable permissions
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&output_path)?.permissions();
            perms.set_mode(0o755);
            
            log::info!("Extracted ffmpeg to {:?}", output_path);
            ffmpeg_extracted = true;
        } else if entry_path.file_name().map_or(false, |name| name == "ffprobe") {
            let output_path = extract_to.join("ffprobe");
            
            // Extract the file
            let mut outfile = std::fs::File::create(&output_path)?;
            std::io::copy(&mut entry, &mut outfile)?;
            
            // Set executable permissions
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&output_path)?.permissions();
            perms.set_mode(0o755);
            
            log::info!("Extracted ffprobe to {:?}", output_path);
            ffprobe_extracted = true;
        }
        
        if ffmpeg_extracted && ffprobe_extracted {
            break; // Both found, no need to continue
        }
    }
    
    if !ffmpeg_extracted {
        return Err(anyhow::anyhow!("ffmpeg binary not found in archive"));
    }
    if !ffprobe_extracted {
        return Err(anyhow::anyhow!("ffprobe binary not found in archive"));
    }
    
    Ok(())
}