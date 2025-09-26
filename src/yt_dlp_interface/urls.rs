pub fn get_latest_yt_dlp_url() -> String {
    let os = if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else if cfg!(target_os = "linux") {
        "yt-dlp_linux"
    } else if cfg!(target_os = "macos") {
        "yt-dlp_macos"
    } else {
        "yt-dlp"  // fallback
    };
    
    // This downloads the latest release from GitHub
    format!("https://github.com/yt-dlp/yt-dlp/releases/latest/download/{}", os)
}

pub fn get_latest_ffmpeg_url() -> String {
    if cfg!(target_os = "windows") {
        "https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/ffmpeg-master-latest-win64-gpl.zip".to_string()
    } else if cfg!(target_os = "linux") {
        // For Linux (including Android/Termux), we'll use a different approach
        // as static builds for Android might need to be handled differently
        "https://johnvansickle.com/ffmpeg/releases/ffmpeg-git-amd64-static.tar.xz".to_string()
    } else {
        "https://evermeet.cx/ffmpeg/get/ffmpeg/7z".to_string() // For macOS as fallback
    }
}