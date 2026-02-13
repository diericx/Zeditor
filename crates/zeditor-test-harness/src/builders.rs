use std::path::PathBuf;
use std::time::Duration;

use uuid::Uuid;
use zeditor_core::media::{MediaAsset, SourceLibrary};
use zeditor_core::project::Project;
use zeditor_core::timeline::{Clip, TimeRange, Timeline, TimelinePosition, Track};

/// Builder for creating test MediaAssets with sensible defaults.
pub struct MediaAssetBuilder {
    name: String,
    path: PathBuf,
    duration: Duration,
    width: u32,
    height: u32,
    fps: f64,
    has_audio: bool,
}

impl MediaAssetBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            path: PathBuf::from(format!("/test/{name}.mp4")),
            duration: Duration::from_secs(10),
            width: 1920,
            height: 1080,
            fps: 30.0,
            has_audio: true,
        }
    }

    pub fn duration_secs(mut self, secs: f64) -> Self {
        self.duration = Duration::from_secs_f64(secs);
        self
    }

    pub fn resolution(mut self, w: u32, h: u32) -> Self {
        self.width = w;
        self.height = h;
        self
    }

    pub fn fps(mut self, fps: f64) -> Self {
        self.fps = fps;
        self
    }

    pub fn no_audio(mut self) -> Self {
        self.has_audio = false;
        self
    }

    pub fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = path.into();
        self
    }

    pub fn build(self) -> MediaAsset {
        MediaAsset::new(
            self.name,
            self.path,
            self.duration,
            self.width,
            self.height,
            self.fps,
            self.has_audio,
        )
    }
}

/// Builder for creating test Clips with sensible defaults.
pub struct ClipBuilder {
    asset_id: Uuid,
    timeline_start_secs: f64,
    source_start_secs: f64,
    duration_secs: f64,
}

impl ClipBuilder {
    pub fn new(asset_id: Uuid) -> Self {
        Self {
            asset_id,
            timeline_start_secs: 0.0,
            source_start_secs: 0.0,
            duration_secs: 5.0,
        }
    }

    pub fn at(mut self, start_secs: f64) -> Self {
        self.timeline_start_secs = start_secs;
        self
    }

    pub fn source_start(mut self, secs: f64) -> Self {
        self.source_start_secs = secs;
        self
    }

    pub fn duration_secs(mut self, secs: f64) -> Self {
        self.duration_secs = secs;
        self
    }

    pub fn build(self) -> Clip {
        let source_range = TimeRange::new(
            TimelinePosition::from_secs_f64(self.source_start_secs),
            TimelinePosition::from_secs_f64(self.source_start_secs + self.duration_secs),
        )
        .expect("invalid source range in test builder");

        Clip::new(
            self.asset_id,
            TimelinePosition::from_secs_f64(self.timeline_start_secs),
            source_range,
        )
    }
}

/// Build a project with a pre-populated source library and timeline.
pub struct ProjectBuilder {
    name: String,
    assets: Vec<MediaAsset>,
    tracks: Vec<Track>,
}

impl ProjectBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            assets: Vec::new(),
            tracks: Vec::new(),
        }
    }

    pub fn with_asset(mut self, asset: MediaAsset) -> Self {
        self.assets.push(asset);
        self
    }

    pub fn with_track(mut self, track: Track) -> Self {
        self.tracks.push(track);
        self
    }

    pub fn build(self) -> Project {
        let mut project = Project::new(&self.name);
        project.source_library = SourceLibrary::new();
        for asset in self.assets {
            project.source_library.import(asset);
        }
        if !self.tracks.is_empty() {
            project.timeline = Timeline {
                tracks: self.tracks,
            };
        }
        project
    }
}
