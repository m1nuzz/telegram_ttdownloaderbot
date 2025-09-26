use std::path::PathBuf;
use tokio::fs;
use tokio::io;
use zip::ZipArchive;
use anyhow::Result;
use std::io::Read;

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