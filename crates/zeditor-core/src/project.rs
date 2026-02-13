use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::commands::CommandHistory;
use crate::error::Result;
use crate::media::SourceLibrary;
use crate::timeline::Timeline;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub timeline: Timeline,
    pub source_library: SourceLibrary,
    #[serde(skip)]
    pub command_history: CommandHistory,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        let mut timeline = Timeline::new();
        timeline.add_track("Video 1");
        timeline.add_track("Audio 1");

        Self {
            name: name.into(),
            timeline,
            source_library: SourceLibrary::new(),
            command_history: CommandHistory::new(),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let json = fs::read_to_string(path)?;
        let project: Self = serde_json::from_str(&json)?;
        Ok(project)
    }
}

impl Default for Project {
    fn default() -> Self {
        Self::new("Untitled")
    }
}
