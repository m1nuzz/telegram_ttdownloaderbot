use std::path::PathBuf;
use tokio::fs;
use tokio::io;
use zip::ZipArchive;
use anyhow::Result;
use std::io::Read;

pub async fn download_file(url: &str, path: &PathBuf) -> Result<()> {
    log::info!("Downloading from {} to {:?}", url, path);
    
    let client = reqwest::Client::new();
    let mut response = client.get(url).send().await?;
    
    let mut file = fs::File::create(path).await?;
    
    // Read the response body in chunks and write to the file
    while let Some(chunk) = response.chunk().await? {
        io::copy(&mut chunk.as_ref(), &mut file).await?;
    }
    
    log::info!("Download completed successfully to {:?}", path);
    Ok(())
}

pub async fn extract_ffmpeg_windows(zip_path: &PathBuf, extract_to: &PathBuf) -> Result<()> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    
    // Find ffmpeg.exe in the archive
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file.name().ends_with("ffmpeg.exe") {
            let outpath = extract_to.join("ffmpeg.exe");
            
            let mut outfile = fs::File::create(&outpath).await?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            io::copy(&mut buffer.as_slice(), &mut outfile).await?;
            
            log::info!("Extracted ffmpeg.exe to {:?}", outpath);
            break;
        }
    }
    
    Ok(())
}