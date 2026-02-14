use std::path::{Path, PathBuf};
use std::process::Command;

/// Generate a small test video using ffmpeg's lavfi test source.
/// Returns the path to the generated file.
pub fn generate_test_video(output_dir: &Path, name: &str, duration_secs: f64) -> PathBuf {
    let output_path = output_dir.join(format!("{name}.mp4"));

    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc=duration={duration_secs}:size=320x240:rate=30"),
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-preset",
            "ultrafast",
        ])
        .arg(&output_path)
        .stderr(std::process::Stdio::null())
        .status()
        .expect("ffmpeg must be installed to generate test fixtures");

    assert!(
        status.success(),
        "ffmpeg failed to generate test video {name}"
    );
    assert!(output_path.exists(), "test video was not created: {name}");

    output_path
}

/// Generate a test video with a specific resolution.
pub fn generate_test_video_with_size(
    output_dir: &Path,
    name: &str,
    duration_secs: f64,
    width: u32,
    height: u32,
) -> PathBuf {
    let output_path = output_dir.join(format!("{name}.mp4"));

    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc=duration={duration_secs}:size={width}x{height}:rate=30"),
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-preset",
            "ultrafast",
        ])
        .arg(&output_path)
        .stderr(std::process::Stdio::null())
        .status()
        .expect("ffmpeg must be installed to generate test fixtures");

    assert!(
        status.success(),
        "ffmpeg failed to generate test video {name}"
    );
    assert!(output_path.exists(), "test video was not created: {name}");

    output_path
}

/// Generate a test video with audio.
pub fn generate_test_video_with_audio(
    output_dir: &Path,
    name: &str,
    duration_secs: f64,
) -> PathBuf {
    let output_path = output_dir.join(format!("{name}.mp4"));

    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc=duration={duration_secs}:size=320x240:rate=30"),
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency=440:duration={duration_secs}"),
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-preset",
            "ultrafast",
            "-c:a",
            "aac",
            "-shortest",
        ])
        .arg(&output_path)
        .stderr(std::process::Stdio::null())
        .status()
        .expect("ffmpeg must be installed to generate test fixtures");

    assert!(
        status.success(),
        "ffmpeg failed to generate test video with audio {name}"
    );

    output_path
}

/// Generate a test video with rotation metadata (display matrix).
/// Creates a base MP4 then remuxes with `-display_rotation` to produce a MOV
/// that has the rotation set in the stream side data, similar to phone-recorded videos.
pub fn generate_test_video_rotated(
    output_dir: &Path,
    name: &str,
    duration_secs: f64,
    width: u32,
    height: u32,
    rotation: u32,
) -> PathBuf {
    // First generate a base video
    let base_path = output_dir.join(format!("{name}_base.mp4"));
    let output_path = output_dir.join(format!("{name}.mov"));

    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc=duration={duration_secs}:size={width}x{height}:rate=30"),
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-preset",
            "ultrafast",
        ])
        .arg(&base_path)
        .stderr(std::process::Stdio::null())
        .status()
        .expect("ffmpeg must be installed to generate test fixtures");

    assert!(status.success(), "ffmpeg failed to generate base video {name}");

    // Remux with display_rotation to add rotation metadata.
    // The -display_rotation flag takes CCW degrees, but our `rotation` parameter
    // uses the phone convention (CW degrees to apply for display), so we negate.
    let ccw_rotation = -(rotation as i32);
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-display_rotation",
            &ccw_rotation.to_string(),
            "-i",
        ])
        .arg(&base_path)
        .args(["-c", "copy"])
        .arg(&output_path)
        .stderr(std::process::Stdio::null())
        .status()
        .expect("ffmpeg must be installed to generate test fixtures");

    assert!(
        status.success(),
        "ffmpeg failed to add rotation to video {name}"
    );
    assert!(output_path.exists(), "rotated video was not created: {name}");

    output_path
}

/// Get a temporary directory for test fixtures that persists for the test run.
pub fn fixture_dir() -> tempfile::TempDir {
    tempfile::TempDir::new().expect("failed to create temp dir for fixtures")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_test_video() {
        let dir = fixture_dir();
        let path = generate_test_video(dir.path(), "test_basic", 1.0);
        assert!(path.exists());
        let metadata = std::fs::metadata(&path).unwrap();
        assert!(metadata.len() > 0, "generated video should not be empty");
    }

    #[test]
    fn test_generate_test_video_with_size() {
        let dir = fixture_dir();
        let path = generate_test_video_with_size(dir.path(), "test_sized", 1.0, 500, 500);
        assert!(path.exists());
        let metadata = std::fs::metadata(&path).unwrap();
        assert!(metadata.len() > 0, "generated video should not be empty");
    }

    #[test]
    fn test_generate_test_video_rotated() {
        let dir = fixture_dir();
        let path = generate_test_video_rotated(dir.path(), "test_rotated", 1.0, 320, 240, 90);
        assert!(path.exists());
        let metadata = std::fs::metadata(&path).unwrap();
        assert!(metadata.len() > 0, "generated video should not be empty");
    }

    #[test]
    fn test_generate_test_video_with_audio() {
        let dir = fixture_dir();
        let path = generate_test_video_with_audio(dir.path(), "test_audio", 1.0);
        assert!(path.exists());
        let metadata = std::fs::metadata(&path).unwrap();
        assert!(metadata.len() > 0);
    }
}
