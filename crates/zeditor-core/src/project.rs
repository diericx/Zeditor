use std::fs;
use std::path::Path;

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::commands::CommandHistory;
use crate::error::{CoreError, Result};
use crate::media::SourceLibrary;
use crate::timeline::{Timeline, TrackType};

/// Project-level settings defining the editing canvas and default framerate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectSettings {
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub fps: f64,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            canvas_width: 1920,
            canvas_height: 1080,
            fps: 30.0,
        }
    }
}

/// Current version written into new save files.
pub const CURRENT_PROJECT_VERSION: &str = "1.0.0";

/// Minimum project file version this app can load.
/// Files older than this are incompatible and require migration (not yet implemented).
pub const MIN_PROJECT_VERSION: &str = "1.0.0";

/// On-disk envelope that wraps a `Project` with a semver version string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectFile {
    pub version: String,
    pub project: Project,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub timeline: Timeline,
    pub source_library: SourceLibrary,
    #[serde(default)]
    pub settings: ProjectSettings,
    #[serde(skip)]
    pub command_history: CommandHistory,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        let mut timeline = Timeline::new();
        timeline.add_track("V1", TrackType::Video);
        timeline.add_track("A1", TrackType::Audio);

        Self {
            name: name.into(),
            timeline,
            source_library: SourceLibrary::new(),
            settings: ProjectSettings::default(),
            command_history: CommandHistory::new(),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let file = ProjectFile {
            version: CURRENT_PROJECT_VERSION.to_string(),
            project: self.clone(),
        };
        let json = serde_json::to_string_pretty(&file)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let json = fs::read_to_string(path)?;

        // Two-pass load: first extract version, then validate, then deserialize.
        let raw: serde_json::Value = serde_json::from_str(&json)?;

        let version_str = raw
            .get("version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                CoreError::InvalidProjectFile("missing or invalid 'version' field".into())
            })?;

        let file_version = Version::parse(version_str).map_err(|e| {
            CoreError::InvalidProjectFile(format!("invalid version '{version_str}': {e}"))
        })?;

        let current = Version::parse(CURRENT_PROJECT_VERSION).unwrap();
        let min = Version::parse(MIN_PROJECT_VERSION).unwrap();

        if file_version > current {
            return Err(CoreError::VersionTooNew {
                got: file_version.to_string(),
                max: current.to_string(),
            });
        }

        if file_version < min {
            return Err(CoreError::VersionTooOld {
                got: file_version.to_string(),
                min: min.to_string(),
            });
        }

        // Future: match on file_version.major to transform `raw` before deserializing.
        let project_file: ProjectFile = serde_json::from_value(raw)?;
        Ok(project_file.project)
    }
}

impl PartialEq for Project {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.timeline == other.timeline
            && self.source_library == other.source_library
            && self.settings == other.settings
    }
}

impl Default for Project {
    fn default() -> Self {
        Self::new("Untitled")
    }
}
