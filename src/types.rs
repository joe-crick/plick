//! Domain-specific newtypes that enforce constraints at the type level.
//!
//! Invalid values cannot be constructed — no runtime assertions needed.

use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

/// Frames per second. Guaranteed non-zero by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Fps(NonZeroU32);

impl Fps {
    /// Create a new `Fps` value. Returns `None` if `value` is zero.
    pub fn new(value: u32) -> Option<Self> {
        NonZeroU32::new(value).map(Self)
    }

    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl std::fmt::Display for Fps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A PipeWire node identifier. Guaranteed non-zero by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeId(NonZeroU32);

impl NodeId {
    /// Create a new `NodeId`. Returns `None` if `value` is zero.
    pub fn new(value: u32) -> Option<Self> {
        NonZeroU32::new(value).map(Self)
    }

    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A rectangular region on screen, in pixels.
/// Used to crop the full-monitor capture to the overlay's area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureRegion {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl CaptureRegion {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Option<Self> {
        if width > 0 && height > 0 {
            Some(Self { x, y, width, height })
        } else {
            None
        }
    }

    /// FFmpeg crop filter string: `crop=W:H:X:Y`
    pub fn to_ffmpeg_crop(&self) -> String {
        format!("crop={}:{}:{}:{}", self.width, self.height, self.x, self.y)
    }
}

impl std::fmt::Display for CaptureRegion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fps_rejects_zero() {
        assert!(Fps::new(0).is_none());
    }

    #[test]
    fn fps_accepts_positive() {
        let fps = Fps::new(30).unwrap();
        assert_eq!(fps.get(), 30);
    }

    #[test]
    fn fps_roundtrips_through_serde() {
        let fps = Fps::new(15).unwrap();
        let json = serde_json::to_string(&fps).unwrap();
        assert_eq!(json, "15");
        let back: Fps = serde_json::from_str(&json).unwrap();
        assert_eq!(back, fps);
    }

    #[test]
    fn fps_serde_rejects_zero() {
        let result: Result<Fps, _> = serde_json::from_str("0");
        assert!(result.is_err());
    }

    #[test]
    fn node_id_rejects_zero() {
        assert!(NodeId::new(0).is_none());
    }

    #[test]
    fn node_id_accepts_positive() {
        let id = NodeId::new(42).unwrap();
        assert_eq!(id.get(), 42);
    }

    #[test]
    fn capture_region_rejects_zero_size() {
        assert!(CaptureRegion::new(0, 0, 0, 100).is_none());
        assert!(CaptureRegion::new(0, 0, 100, 0).is_none());
    }

    #[test]
    fn capture_region_accepts_valid() {
        let r = CaptureRegion::new(100, 200, 640, 480).unwrap();
        assert_eq!(r.x, 100);
        assert_eq!(r.y, 200);
        assert_eq!(r.width, 640);
        assert_eq!(r.height, 480);
    }

    #[test]
    fn capture_region_ffmpeg_crop() {
        let r = CaptureRegion::new(100, 200, 640, 480).unwrap();
        assert_eq!(r.to_ffmpeg_crop(), "crop=640:480:100:200");
    }

    #[test]
    fn capture_region_display() {
        let r = CaptureRegion::new(0, 0, 800, 600).unwrap();
        assert_eq!(format!("{}", r), "800x600");
    }
}
