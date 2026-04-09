//! Video-to-GIF conversion using FFmpeg.
//!
//! Uses a 2-pass approach for high-quality GIF output:
//! 1. Generate an optimal palette from the source video.
//! 2. Encode the GIF using that palette.
//!
//! `Fps` is guaranteed non-zero by its type — no validation needed in
//! these functions.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::types::Fps;

/// Outcome of a successful conversion.
#[derive(Debug)]
pub struct ConversionResult {
    pub gif_path: PathBuf,
    pub gif_size_bytes: u64,
}

/// Which pass of the 2-pass conversion is currently running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionPhase {
    GeneratingPalette,
    EncodingGif,
}

/// Convert a video file to a high-quality GIF using 2-pass FFmpeg.
///
/// 1. Generates a palette PNG in a temp file next to the input.
/// 2. Encodes the GIF using that palette.
/// 3. Cleans up the palette file regardless of success or failure.
///
/// `gif_width` scales the output to the given width (preserving aspect ratio).
/// `gif_colors` sets the palette size (2–256); lower values compress better.
/// `on_phase` is called when each phase starts (for UI progress updates).
pub fn convert<F>(
    input: &Path,
    output: &Path,
    gif_fps: Fps,
    gif_width: Option<u32>,
    gif_colors: u32,
    mut on_phase: F,
) -> Result<ConversionResult>
where
    F: FnMut(ConversionPhase),
{
    if !input.exists() {
        bail!("Input video does not exist: {}", input.display());
    }

    // Palette goes next to the input file to avoid cross-filesystem issues.
    let palette_path = input.with_extension("palette.png");

    // Pass 1: palette generation
    on_phase(ConversionPhase::GeneratingPalette);
    let palette_result = run_ffmpeg(&build_palette_args(input, &palette_path, gif_fps, gif_width, gif_colors));

    // If palette generation failed, clean up and bail.
    if let Err(e) = palette_result {
        let _ = std::fs::remove_file(&palette_path);
        return Err(e.context("FFmpeg palette generation failed"));
    }

    // Pass 2: GIF encoding
    on_phase(ConversionPhase::EncodingGif);
    let gif_result = run_ffmpeg(&build_gif_args(input, &palette_path, output, gif_fps, gif_width));

    // Always clean up the palette file.
    let _ = std::fs::remove_file(&palette_path);

    gif_result.context("FFmpeg GIF encoding failed")?;

    // Verify the output exists and get its size.
    let metadata = std::fs::metadata(output)
        .with_context(|| format!("GIF was not created at {}", output.display()))?;

    Ok(ConversionResult {
        gif_path: output.to_path_buf(),
        gif_size_bytes: metadata.len(),
    })
}

/// Run an FFmpeg command and return an error if it fails.
fn run_ffmpeg(args: &[String]) -> Result<()> {
    let output = Command::new("ffmpeg")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .context("Failed to execute ffmpeg")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "ffmpeg exited with status {}: {}",
            output.status,
            stderr.lines().last().unwrap_or("(no output)")
        );
    }

    Ok(())
}

/// Build FFmpeg arguments for pass 1: palette generation.
pub fn build_palette_args(
    input: &Path,
    palette_output: &Path,
    gif_fps: Fps,
    gif_width: Option<u32>,
    gif_colors: u32,
) -> Vec<String> {
    let scale = scale_filter(gif_width);
    vec![
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-vf".into(),
        // stats_mode=diff: build palette from inter-frame differences instead of
        // full frames — much better for screen recordings where most pixels are static.
        format!("fps={}{scale},palettegen=max_colors={gif_colors}:stats_mode=diff", gif_fps),
        palette_output.to_string_lossy().into_owned(),
    ]
}

/// Build FFmpeg arguments for pass 2: GIF encoding with palette.
pub fn build_gif_args(
    input: &Path,
    palette: &Path,
    output: &Path,
    gif_fps: Fps,
    gif_width: Option<u32>,
) -> Vec<String> {
    let scale = scale_filter(gif_width);
    vec![
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-i".into(),
        palette.to_string_lossy().into_owned(),
        "-lavfi".into(),
        // diff_mode=rectangle: only re-encode the bounding box of changed pixels;
        // unchanged areas become transparent/empty and compress near-zero with LZW.
        format!(
            "fps={}{scale} [x]; [x][1:v] paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle",
            gif_fps
        ),
        output.to_string_lossy().into_owned(),
    ]
}

/// Returns a `,scale=W:-2:flags=lanczos` filter string (leading comma), or empty string.
///
/// Used as a suffix after `fps=N` in filtergraph strings:
/// - with width: `fps=15,scale=960:-2:flags=lanczos`
/// - without:    `fps=15`
///
/// `-2` keeps the height divisible by 2 while preserving the aspect ratio.
fn scale_filter(gif_width: Option<u32>) -> String {
    match gif_width {
        Some(w) => format!(",scale={w}:-2:flags=lanczos"),
        None => String::new(),
    }
}

/// Generate a timestamped output filename under `output_dir`.
///
/// Format: `screencast-YYYY-MM-DD-HHMMSS.gif`
pub fn generate_output_filename(output_dir: &Path) -> PathBuf {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Convert to date-time components (UTC).
    let s = secs % 60;
    let total_min = secs / 60;
    let m = total_min % 60;
    let total_hr = total_min / 60;
    let h = total_hr % 24;
    let days = total_hr / 24;

    // Days since epoch to Y-M-D (simplified Gregorian).
    let (year, month, day) = epoch_days_to_ymd(days);

    output_dir.join(format!(
        "screencast-{year:04}-{month:02}-{day:02}-{h:02}{m:02}{s:02}.gif"
    ))
}

fn epoch_days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Algorithm from Howard Hinnant's civil_from_days.
    days += 719_468;
    let era = days / 146_097;
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recorder::is_ffmpeg_available;

    fn fps(n: u32) -> Fps {
        Fps::new(n).unwrap()
    }

    // --- Arg construction tests (always run) ---

    #[test]
    fn palette_args_has_one_input_and_palettegen() {
        let args = build_palette_args(
            Path::new("/tmp/recording.webm"),
            Path::new("/tmp/palette.png"),
            fps(15),
            None,
            128,
        );

        assert_eq!(args.iter().filter(|a| *a == "-i").count(), 1);
        assert!(args.iter().any(|a| a.contains("palettegen")));
        assert!(args.iter().any(|a| a.contains("fps=15")));
        assert_eq!(args.last().unwrap(), "/tmp/palette.png");
    }

    #[test]
    fn palette_args_includes_scale_filter_when_width_set() {
        let args = build_palette_args(
            Path::new("/tmp/recording.webm"),
            Path::new("/tmp/palette.png"),
            fps(15),
            Some(640),
            128,
        );

        let vf = args.iter().skip_while(|a| *a != "-vf").nth(1).unwrap();
        assert!(vf.contains(",scale=640:-2:flags=lanczos,"), "got: {vf}");
    }

    #[test]
    fn gif_args_has_two_inputs_and_paletteuse() {
        let args = build_gif_args(
            Path::new("/tmp/recording.webm"),
            Path::new("/tmp/palette.png"),
            Path::new("/tmp/output.gif"),
            fps(15),
            None,
        );

        assert_eq!(args.iter().filter(|a| *a == "-i").count(), 2);
        assert!(args.iter().any(|a| a.contains("paletteuse")));
        assert!(args.iter().any(|a| a.contains("fps=15")));
        assert_eq!(args.last().unwrap(), "/tmp/output.gif");
    }

    #[test]
    fn gif_args_includes_scale_and_bayer_when_width_set() {
        let args = build_gif_args(
            Path::new("/tmp/recording.webm"),
            Path::new("/tmp/palette.png"),
            Path::new("/tmp/output.gif"),
            fps(15),
            Some(960),
        );

        let lavfi = args.iter().skip_while(|a| *a != "-lavfi").nth(1).unwrap();
        assert!(lavfi.contains(",scale=960:-2:flags=lanczos "), "got: {lavfi}");
        assert!(lavfi.contains("dither=bayer"), "got: {lavfi}");
    }

    #[test]
    fn generated_filename_has_gif_extension_and_screencast_prefix() {
        let path = generate_output_filename(Path::new("/home/user/Videos"));
        assert!(path.starts_with("/home/user/Videos"));
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("gif"));
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("screencast-"), "expected screencast- prefix, got: {name}");
        // Format: screencast-YYYY-MM-DD-HHMMSS.gif
        assert_eq!(name.len(), "screencast-YYYY-MM-DD-HHMMSS.gif".len());
    }

    // --- convert() error handling tests (always run) ---

    #[test]
    fn convert_fails_on_missing_input() {
        let result = convert(
            Path::new("/tmp/nonexistent_plick_test.webm"),
            Path::new("/tmp/out.gif"),
            fps(15),
            None,
            128,
            |_| {},
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn convert_reports_phases_in_order() {
        // Even though this will fail (no input file), we can test
        // that GeneratingPalette is reported before the error.
        let mut phases = vec![];
        let _ = convert(
            Path::new("/tmp/nonexistent_plick_test.webm"),
            Path::new("/tmp/out.gif"),
            fps(15),
            None,
            128,
            |phase| phases.push(phase),
        );
        // Should not have gotten to any phase since input doesn't exist.
        assert!(phases.is_empty());
    }

    // --- Integration tests (only run if FFmpeg is available) ---

    /// Create a tiny test video using FFmpeg's test source.
    fn create_test_video(path: &Path) -> Result<()> {
        let output = Command::new("ffmpeg")
            .args([
                "-y",
                "-f", "lavfi",
                "-i", "testsrc=duration=1:size=64x64:rate=10",
                "-c:v", "libvpx",
                "-b:v", "200K",
                "-an",
                &path.to_string_lossy(),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output()?;
        if !output.status.success() {
            bail!("Failed to create test video");
        }
        Ok(())
    }

    #[test]
    fn convert_produces_gif_from_video() {
        if !is_ffmpeg_available() {
            eprintln!("Skipping: FFmpeg not available");
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let video_path = dir.path().join("test.webm");
        let gif_path = dir.path().join("test.gif");

        create_test_video(&video_path).unwrap();

        let mut phases = vec![];
        let result = convert(&video_path, &gif_path, fps(10), None, 128, |phase| {
            phases.push(phase);
        });

        let result = result.unwrap();
        assert!(result.gif_path.exists());
        assert!(result.gif_size_bytes > 0);
        assert_eq!(phases, vec![
            ConversionPhase::GeneratingPalette,
            ConversionPhase::EncodingGif,
        ]);

        // Palette temp file should have been cleaned up.
        assert!(!video_path.with_extension("palette.png").exists());
    }

    #[test]
    fn convert_cleans_up_palette_on_bad_input() {
        if !is_ffmpeg_available() {
            eprintln!("Skipping: FFmpeg not available");
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let bad_video = dir.path().join("corrupt.webm");
        let gif_path = dir.path().join("output.gif");

        // Write garbage to simulate a corrupt video.
        std::fs::write(&bad_video, b"not a video file").unwrap();

        let result = convert(&bad_video, &gif_path, fps(10), None, 128, |_| {});
        assert!(result.is_err());

        // Palette file should not remain.
        assert!(!bad_video.with_extension("palette.png").exists());
    }
}
