/// The app version shown in the Help window's footer.
pub const APP_VERSION: &str = "0.3";

/// Audio quality settings for exported files. Change these to alter the
/// output quality of both the .wav and .mp3 exports.
pub const SAMPLE_RATE_HZ: u32 = 48_000;
pub const MP3_BITRATE_KBPS: u32 = 320;
pub const WAV_BITS_PER_SAMPLE: u16 = 32;

/// Where exported files are written.
#[allow(dead_code)]
pub enum ExportDir {
    /// The user's platform-specific Downloads folder.
    Downloads,
    /// A fixed filesystem path, used as-is.
    Path(&'static str),
}

pub const EXPORT_DIR: ExportDir = ExportDir::Downloads;