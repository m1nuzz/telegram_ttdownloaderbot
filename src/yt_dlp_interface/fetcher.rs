use std::path::PathBuf;
use tokio::process::Command;
use tokio::io::{BufReader, AsyncBufReadExt};
use anyhow::Result;
use regex::Regex;

use crate::utils::progress_bar::ProgressBar;

#[derive(Clone)]
pub struct YoutubeFetcher {
    pub yt_dlp_path: PathBuf,
    pub output_dir: PathBuf,
    pub ffmpeg_dir: PathBuf,
}

impl YoutubeFetcher {
    pub fn new(yt_dlp_path: PathBuf, output_dir: PathBuf, ffmpeg_dir: PathBuf) -> Result<Self> {
        Ok(YoutubeFetcher {
            yt_dlp_path,
            output_dir,
            ffmpeg_dir,
        })
    }

pub async fn download_video_from_url(&self,url: String,filename_stem: &str,quality: &str,progress_bar: &mut ProgressBar) -> Result<std::path::PathBuf> {
        let output_template = if quality == "audio" {
            self.output_dir.join(format!("{}.%(ext)s", filename_stem))
        } else {
            self.output_dir.join(format!("{}.mp4", filename_stem))
        };

        let mut cmd = Command::new(&self.yt_dlp_path);
        cmd.arg("--extractor-args")
           .arg("tiktok:skip=feed")
           .arg("--output")
           .arg(&output_template)
           .arg("--no-part")
           .arg("--no-mtime")
           .arg("--ffmpeg-location")
           .arg(&self.ffmpeg_dir)
           .arg(&url)
           .arg("--progress")
           .arg("--newline")
           .stdout(std::process::Stdio::piped())
           .stderr(std::process::Stdio::piped());

        match quality {
            "h264" => {
                cmd.arg("--format").arg("bestvideo[vcodec=h264]+bestaudio/best[vcodec=h264]");
            }
            "h265" => {
                cmd.arg("--format").arg("bestvideo[vcodec=h265]+bestaudio/best[vcodec=h265]");
            }
            "audio" => {
                cmd.arg("--extract-audio").arg("--audio-format").arg("mp3");
            }
            _ => {
                cmd.arg("--format").arg("best");
            }
        }

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().expect("stdout not captured");
        let stderr = child.stderr.take().expect("stderr not captured");

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut last_percentage = 0.0f64;

        loop {
            tokio::select! {
                line = stdout_reader.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            log::trace!("yt-dlp stdout: {}", line);
                            if let Some((percentage, total_size)) = parse_progress_line(&line) {
                                if percentage > last_percentage {
                                    last_percentage = percentage;
                                    // KEY CHANGE: scale 0-100% yt-dlp to 0-80% of overall progress
                                    let overall_percentage = (percentage * 0.8) as u8; // 0-80%
                                    let info = format!("⬇️ Downloading: {:.1}% ({:.1} MB)",percentage, total_size as f64 / 1_048_576.0);
                                    progress_bar.update(overall_percentage, Some(&info)).await?;
                                }
                            }
                        },
                        Ok(None) => break,
                        Err(_) => break,
                    }
                },
                line = stderr_reader.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            log::trace!("yt-dlp stderr: {}", line);
                            if let Some((percentage, total_size)) = parse_progress_line(&line) {
                                if percentage > last_percentage {
                                    last_percentage = percentage;
                                    let current_size = (total_size as f64 * (percentage / 100.0)) as u64;
                                    let overall_percentage = ((current_size as f64 / total_size as f64 * 80.0).min(80.0).max(0.0)) as u8;
                                    let info = format!("⬇️ Downloading: {:.1}% ({:.1} MB)", percentage, total_size as f64 / 1_048_576.0);
                                    progress_bar.update(overall_percentage, Some(&info)).await?;
                                }
                            }
                        },
                        Ok(None) => {},
                        Err(_) => {},
                    }
                }
            }
        }

        let status = child.wait().await?;

        if status.success() {
            // After download completion, show 80%
            progress_bar.update(80, Some("⬇️ Download completed")).await?;
            
            let parent = self.output_dir.clone();
            let stem = PathBuf::from(filename_stem);

            for ext in [".mp4", ".mov", ".webm", ".mkv", ".flv", ".m4a", ".mp3", ".ogg", ".aac"] {
                let alt_path = parent.join(format!("{}{}", stem.to_string_lossy(), ext));
                if alt_path.exists() {
                    return Ok(alt_path);
                }
            }
            Err(anyhow::anyhow!("Downloaded file not found"))
        } else {
            Err(anyhow::anyhow!("yt-dlp failed"))
        }
    }
}

fn parse_progress_line(line: &str) -> Option<(f64, u64)> {
    let clean_line = remove_ansi_codes(line);
    let patterns = [
        r"\[download\]\s+(\d+\.?\d*)%\s+of\s+(\d+\.?\d*[KMGT]?i?B)",
        r"\[download\]\s+(\d+\.?\d*)%\s+of\s+~(\d+\.?\d*[KMGT]?i?B)",
        r"(\d+\.?\d*)%",
    ];

    for pattern in patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(caps) = re.captures(&clean_line) {
                if let Ok(percentage) = caps[1].parse::<f64>() {
                    let total_size = if caps.len() > 2 {
                        parse_size_string(&caps[2])
                    } else {
                        10_485_760
                    };
                    return Some((percentage, total_size));
                }
            }
        }
    }
    None
}

fn remove_ansi_codes(text: &str) -> String {
    let re = Regex::new(r"\x1B\[[0-?]*[ -/]*[@-~]").unwrap();
    re.replace_all(text, "").to_string()
}

fn parse_size_string(s: &str) -> u64 {
    let s_clean = s.trim().to_lowercase();
    let (number_str, multiplier) = if s_clean.ends_with("mib") || s_clean.ends_with("mb") {
        (s_clean.trim_end_matches("mib").trim_end_matches("mb"), 1_024 * 1_024)
    } else if s_clean.ends_with("gib") || s_clean.ends_with("gb") {
        (s_clean.trim_end_matches("gib").trim_end_matches("gb"), 1_024 * 1_024 * 1_024)
    } else {
        (s_clean.trim_end_matches("b"), 1_048_576)
    };
    number_str.parse::<f64>().unwrap_or(1.0) as u64 * multiplier
}
