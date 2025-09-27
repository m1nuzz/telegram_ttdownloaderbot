# Telegram TikTok Downloader Bot

A Telegram bot written in Rust for downloading TikTok videos without watermarks, with support for various social media platforms including TikTok, Instagram, YouTube Shorts, and more.

## Features

- **Multi-platform support**: Download videos from TikTok, Instagram, YouTube Shorts, and other social media platforms
- **No watermarks**: Download clean videos without watermarks
- **High-quality downloads**: Automatically selects the best available quality
- **Telegram interface**: Easy-to-use bot interface within Telegram
- **Automatic updates**: Built-in auto-update functionality for yt-dlp and FFmpeg binaries
- **Database support**: Stores user information and download history
- **Admin commands**: Administrative features for channel management
- **Cross-platform**: Runs on Windows, Linux, and macOS

## Auto-Update Functionality

The bot features an RSS-based auto-update system that:
- Monitors GitHub releases for yt-dlp and FFmpeg
- Automatically downloads and installs new versions
- Checks for updates every 2 hours
- Maintains version tracking to avoid unnecessary updates
- Handles platform-specific binary extraction

## Dependencies

- Rust 1.89.0 or later
- Telegram Bot API token
- SQLite (for database)
- yt-dlp and FFmpeg (automatically downloaded on first run)

## Commands

- `/start`: Start the bot
- `/help`: Show help information
- Simply send a TikTok/Instagram/YouTube link to download the video

## Configuration

The bot can be configured using environment variables in the `.env` file:
- `TELOXIDE_TOKEN`: Your Telegram bot token
- `DATABASE_PATH`: Path to the SQLite database file

## Contributing

Contributions are welcome! Please feel free to fork the repository and submit pull requests.

## License

This project is licensed under the MIT License.
