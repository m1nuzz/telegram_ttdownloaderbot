pub mod fetcher;
pub mod utils;
pub mod urls;
pub mod downloader;
pub mod ensure;

pub use fetcher::YoutubeFetcher;
pub use utils::is_executable_present;
pub use ensure::ensure_binaries;
pub use downloader::download_file;

#[cfg(target_os = "windows")]
pub use downloader::extract_ffmpeg_windows;

#[cfg(target_os = "macos")]
pub use downloader::extract_ffmpeg_macos;

#[cfg(all(unix, not(target_os = "macos")))]
pub use downloader::extract_ffmpeg_unix;
