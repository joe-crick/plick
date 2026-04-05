//! Screen recording via xdg-desktop-portal and GStreamer.
//!
//! Uses `ashpd` to request a screencast session from the desktop portal,
//! then spawns a GStreamer subprocess (`gst-launch-1.0`) to capture the
//! PipeWire stream to a temporary `.webm` file.
//!
//! # Design
//!
//! Invalid states are unrepresentable: `RecorderState` is an enum where
//! each variant holds only the data relevant to that state. You cannot
//! access a process handle when idle, or a GIF path before conversion —
//! the types prevent it.
//!
//! The portal returns a PipeWire file descriptor and node ID. We clear
//! CLOEXEC on the fd so the GStreamer child inherits it, and set
//! `PIPEWIRE_REMOTE=<fd>` in its environment.

use anyhow::{bail, Context, Result};
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use crate::config::Config;
use crate::types::{CaptureRegion, Fps, NodeId};

/// The recorder lifecycle. Each variant holds exactly the data it needs.
///
/// ```text
/// Idle → Countdown → Recording → Converting → Preview → Idle
///                      ↕ (cancel at any point returns to Idle)
/// ```
pub enum RecorderState {
    Idle,
    Countdown {
        remaining_secs: u64,
    },
    Recording {
        capture_process: Child,
        temp_video_path: PathBuf,
        started_at: std::time::Instant,
    },
    Converting {
        temp_video_path: PathBuf,
    },
    Preview {
        gif_path: PathBuf,
    },
}

/// Manages screen recording lifecycle.
pub struct Recorder {
    config: Config,
    state: RecorderState,
}

impl Recorder {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            state: RecorderState::Idle,
        }
    }

    /// Returns `true` if currently idle.
    pub fn is_idle(&self) -> bool {
        matches!(self.state, RecorderState::Idle)
    }

    /// Returns `true` if currently recording.
    pub fn is_recording(&self) -> bool {
        matches!(self.state, RecorderState::Recording { .. })
    }

    /// Returns a description of the current state (for UI display).
    pub fn state_label(&self) -> &'static str {
        match &self.state {
            RecorderState::Idle => "Idle",
            RecorderState::Countdown { .. } => "Countdown",
            RecorderState::Recording { .. } => "Recording",
            RecorderState::Converting { .. } => "Converting",
            RecorderState::Preview { .. } => "Preview",
        }
    }

    /// Access the GIF path when in Preview state.
    pub fn preview_gif_path(&self) -> Option<&PathBuf> {
        match &self.state {
            RecorderState::Preview { gif_path } => Some(gif_path),
            _ => None,
        }
    }

    /// Begin countdown. Only valid from `Idle`.
    pub fn begin_countdown(&mut self) -> Result<u64> {
        if !self.is_idle() {
            bail!("Cannot start countdown: recorder is {}", self.state_label());
        }
        let secs = self.config.countdown_secs;
        self.state = RecorderState::Countdown {
            remaining_secs: secs,
        };
        Ok(secs)
    }

    /// Access the config.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Returns elapsed recording time, or `None` if not recording.
    pub fn elapsed(&self) -> Option<std::time::Duration> {
        match &self.state {
            RecorderState::Recording { started_at, .. } => Some(started_at.elapsed()),
            _ => None,
        }
    }

    /// Start recording using a portal-provided PipeWire node ID.
    ///
    /// GStreamer's `pipewiresrc` connects to PipeWire directly — the
    /// portal session authorizes access to the node, so we just need
    /// the node ID, not the fd.
    pub fn start_with_stream(
        &mut self,
        node_id: NodeId,
        crop: Option<CaptureRegion>,
    ) -> Result<()> {
        let can_start = matches!(
            self.state,
            RecorderState::Countdown { .. } | RecorderState::Idle
        );
        if !can_start {
            bail!("Cannot start recording: recorder is {}", self.state_label());
        }

        if !is_gstreamer_available() {
            bail!("gst-launch-1.0 is not available in PATH");
        }

        let temp_path = std::env::temp_dir()
            .join(format!("plick_recording_{}.webm", std::process::id()));

        let args = build_gst_record_args(node_id, &temp_path, self.config.capture_fps, crop);
        let child = spawn_gst(&args).context("Failed to spawn gst-launch-1.0")?;

        self.state = RecorderState::Recording {
            capture_process: child,
            temp_video_path: temp_path,
            started_at: std::time::Instant::now(),
        };

        Ok(())
    }

    /// Start recording: request portal screencast, spawn GStreamer.
    /// Convenience method that combines portal + start_with_stream.
    pub async fn start(&mut self, crop: Option<CaptureRegion>) -> Result<()> {
        let can_start = matches!(
            self.state,
            RecorderState::Countdown { .. } | RecorderState::Idle
        );
        if !can_start {
            bail!("Cannot start recording: recorder is {}", self.state_label());
        }

        let (node_id, _pw_fd) =
            request_screencast().await.context("Failed to request screencast from portal")?;

        self.start_with_stream(node_id, crop)
    }

    /// Stop recording: send SIGINT to gst-launch, return the temp video path.
    /// The `-e` flag makes gst-launch send EOS on interrupt for a clean file.
    /// Transitions to `Idle`. Caller is responsible for the next step (conversion).
    pub fn stop(&mut self) -> Result<PathBuf> {
        // Take ownership of the Recording data by swapping in Idle.
        let prev = std::mem::replace(&mut self.state, RecorderState::Idle);

        match prev {
            RecorderState::Recording {
                mut capture_process,
                temp_video_path,
                started_at: _,
            } => {
                // SIGINT tells gst-launch-1.0 -e to send EOS and finish cleanly.
                let pid = capture_process.id();
                eprintln!("Sending SIGINT to gst-launch (pid {})", pid);
                unsafe {
                    libc::kill(pid as i32, libc::SIGINT);
                }
                // Wait for GStreamer to flush and write the file trailer.
                let stderr_output = capture_process.stderr.take().map(|mut s| {
                    let mut buf = String::new();
                    use std::io::Read;
                    let _ = s.read_to_string(&mut buf);
                    buf
                });
                match capture_process.wait() {
                    Ok(status) => eprintln!("gst-launch exited: {status}"),
                    Err(e) => eprintln!("gst-launch wait error: {e}"),
                }
                if let Some(stderr) = stderr_output {
                    if !stderr.is_empty() {
                        eprintln!("gst-launch stderr:\n{stderr}");
                    }
                }

                Ok(temp_video_path)
            }
            other => {
                // Put the old state back — we didn't actually transition.
                self.state = other;
                bail!("Cannot stop: recorder is {}", self.state_label());
            }
        }
    }

    /// Transition to Converting state. Called after stop() returns a video path.
    pub fn begin_converting(&mut self, temp_video_path: PathBuf) -> Result<()> {
        if !self.is_idle() {
            bail!(
                "Cannot begin converting: recorder is {}",
                self.state_label()
            );
        }
        self.state = RecorderState::Converting { temp_video_path };
        Ok(())
    }

    /// Transition to Preview state. Called after GIF conversion completes.
    pub fn set_preview(&mut self, gif_path: PathBuf) -> Result<()> {
        if !matches!(self.state, RecorderState::Converting { .. }) {
            bail!("Cannot preview: recorder is {}", self.state_label());
        }
        self.state = RecorderState::Preview { gif_path };
        Ok(())
    }

    /// Cancel back to idle from any state. Cleans up FFmpeg if recording.
    pub fn cancel(&mut self) {
        if let RecorderState::Recording {
            ref mut capture_process,
            ..
        } = self.state
        {
            let pid = capture_process.id();
            unsafe {
                libc::kill(pid as i32, libc::SIGINT);
            }
            let _ = capture_process.wait();
        }
        self.state = RecorderState::Idle;
    }
}

// --- Portal ---

/// Request a screencast session from xdg-desktop-portal.
pub async fn request_screencast() -> Result<(NodeId, OwnedFd)> {
    use ashpd::desktop::screencast::{CursorMode, Screencast, SourceType};
    use ashpd::desktop::PersistMode;

    let proxy = Screencast::new()
        .await
        .context("Failed to connect to screencast portal")?;

    let session = proxy
        .create_session()
        .await
        .context("Failed to create screencast session")?;

    proxy
        .select_sources(
            &session,
            CursorMode::Embedded,
            SourceType::Monitor | SourceType::Window,
            false,
            None,
            PersistMode::DoNot,
        )
        .await
        .context("Failed to select screencast sources")?;

    let request = proxy
        .start(&session, None)
        .await
        .context("Screencast start failed (user may have cancelled)")?;

    let response = request
        .response()
        .context("Portal response indicated failure")?;

    let streams = response.streams();
    if streams.is_empty() {
        bail!("Portal returned no streams");
    }

    let raw_id = streams[0].pipe_wire_node_id();
    let node_id =
        NodeId::new(raw_id).context("Portal returned invalid node ID 0")?;

    let pw_fd = proxy
        .open_pipe_wire_remote(&session)
        .await
        .context("Failed to open PipeWire remote fd")?;

    Ok((node_id, pw_fd))
}

// --- External tool checks ---

/// Checks whether FFmpeg is available in PATH (needed for GIF conversion).
pub fn is_ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Checks whether gst-launch-1.0 is available in PATH.
pub fn is_gstreamer_available() -> bool {
    Command::new("gst-launch-1.0")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// --- GStreamer recording ---

/// Build GStreamer arguments for recording from a PipeWire node.
///
/// Uses `pipewiresrc` to read from the portal's PipeWire stream and
/// encodes to VP8/WebM via `vp8enc` + `webmmux`.
/// Each pipeline element/property is a separate argument for gst-launch-1.0.
pub fn build_gst_record_args(
    node_id: NodeId,
    output_path: &PathBuf,
    fps: Fps,
    crop: Option<CaptureRegion>,
) -> Vec<String> {
    let mut args: Vec<String> = vec!["-e".into()];

    // Source: PipeWire
    args.extend([
        "pipewiresrc".into(),
        format!("path={node_id}"),
        "do-timestamp=true".into(),
        "keepalive-time=1000".into(),
        "resend-last=true".into(),
        "!".into(),
    ]);

    // Framerate control
    args.extend([
        "videorate".into(),
        "!".into(),
        format!("video/x-raw,framerate={fps}/1"),
        "!".into(),
        "videoconvert".into(),
    ]);

    // Optional crop
    if let Some(region) = crop {
        args.extend([
            "!".into(),
            "videocrop".into(),
            format!("top={}", region.y),
            format!("left={}", region.x),
            "right=0".into(),
            "bottom=0".into(),
        ]);
    }

    // Encoder + muxer + output
    args.extend([
        "!".into(),
        "vp8enc".into(),
        "min_quantizer=13".into(),
        "max_quantizer=13".into(),
        "cpu-used=5".into(),
        "deadline=1000000".into(),
        "threads=4".into(),
        "!".into(),
        "webmmux".into(),
        "!".into(),
        "filesink".into(),
        format!("location={}", output_path.to_string_lossy()),
    ]);

    args
}

/// Spawn gst-launch-1.0 to record.
fn spawn_gst(args: &[String]) -> Result<Child> {
    eprintln!("GStreamer: gst-launch-1.0 {}", args.join(" "));

    Command::new("gst-launch-1.0")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn gst-launch-1.0")
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- State machine tests ---

    #[test]
    fn new_recorder_is_idle() {
        let r = Recorder::new(Config::default());
        assert!(r.is_idle());
        assert!(!r.is_recording());
        assert_eq!(r.state_label(), "Idle");
    }

    #[test]
    fn begin_countdown_from_idle() {
        let mut r = Recorder::new(Config::default());
        let secs = r.begin_countdown().unwrap();
        assert_eq!(secs, 3);
        assert_eq!(r.state_label(), "Countdown");
    }

    #[test]
    fn begin_countdown_from_non_idle_fails() {
        let mut r = Recorder::new(Config::default());
        r.begin_countdown().unwrap();
        // Already in Countdown — can't start another.
        assert!(r.begin_countdown().is_err());
    }

    #[test]
    fn stop_when_not_recording_returns_error() {
        let mut r = Recorder::new(Config::default());
        let result = r.stop();
        assert!(result.is_err());
        // State should still be Idle (unchanged).
        assert!(r.is_idle());
    }

    #[test]
    fn stop_from_countdown_returns_error_and_preserves_state() {
        let mut r = Recorder::new(Config::default());
        r.begin_countdown().unwrap();
        let result = r.stop();
        assert!(result.is_err());
        assert_eq!(r.state_label(), "Countdown");
    }

    #[test]
    fn cancel_from_idle_stays_idle() {
        let mut r = Recorder::new(Config::default());
        r.cancel();
        assert!(r.is_idle());
    }

    #[test]
    fn cancel_from_countdown_goes_to_idle() {
        let mut r = Recorder::new(Config::default());
        r.begin_countdown().unwrap();
        r.cancel();
        assert!(r.is_idle());
    }

    #[test]
    fn begin_converting_from_idle() {
        let mut r = Recorder::new(Config::default());
        r.begin_converting(PathBuf::from("/tmp/video.webm")).unwrap();
        assert_eq!(r.state_label(), "Converting");
    }

    #[test]
    fn set_preview_from_converting() {
        let mut r = Recorder::new(Config::default());
        r.begin_converting(PathBuf::from("/tmp/video.webm")).unwrap();
        r.set_preview(PathBuf::from("/tmp/output.gif")).unwrap();
        assert_eq!(r.state_label(), "Preview");
        assert_eq!(
            r.preview_gif_path(),
            Some(&PathBuf::from("/tmp/output.gif"))
        );
    }

    #[test]
    fn set_preview_from_idle_fails() {
        let mut r = Recorder::new(Config::default());
        assert!(r.set_preview(PathBuf::from("/tmp/output.gif")).is_err());
    }

    #[test]
    fn preview_gif_path_none_when_not_in_preview() {
        let r = Recorder::new(Config::default());
        assert!(r.preview_gif_path().is_none());
    }

    // --- GStreamer arg construction ---

    #[test]
    fn build_gst_record_args_structure() {
        let node_id = NodeId::new(42).unwrap();
        let fps = Fps::new(30).unwrap();
        let path = PathBuf::from("/tmp/test.webm");
        let args = build_gst_record_args(node_id, &path, fps, None);

        assert_eq!(args[0], "-e");
        assert!(args.contains(&"pipewiresrc".to_string()));
        assert!(args.contains(&"path=42".to_string()));
        assert!(args.contains(&"video/x-raw,framerate=30/1".to_string()));
        assert!(args.contains(&"vp8enc".to_string()));
        assert!(args.contains(&"webmmux".to_string()));
        assert!(args.contains(&"location=/tmp/test.webm".to_string()));
        // No crop when None
        assert!(!args.contains(&"videocrop".to_string()));
    }

    #[test]
    fn build_gst_record_args_with_crop() {
        let node_id = NodeId::new(1).unwrap();
        let fps = Fps::new(30).unwrap();
        let path = PathBuf::from("/tmp/test.webm");
        let crop = CaptureRegion::new(100, 200, 640, 480).unwrap();
        let args = build_gst_record_args(node_id, &path, fps, Some(crop));

        assert!(args.contains(&"videocrop".to_string()));
        assert!(args.contains(&"top=200".to_string()));
        assert!(args.contains(&"left=100".to_string()));
    }

    // Note: no "rejects zero" tests — NodeId::new(0) and Fps::new(0)
    // return None, so you can't even construct the invalid value.

    #[test]
    fn ffmpeg_availability_check_runs() {
        let _available = is_ffmpeg_available();
    }
}
