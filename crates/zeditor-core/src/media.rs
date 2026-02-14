use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaAsset {
    pub id: Uuid,
    pub name: String,
    pub path: PathBuf,
    pub duration: Duration,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub has_audio: bool,
    /// Rotation metadata in degrees (0, 90, 180, 270). Defaults to 0.
    #[serde(default)]
    pub rotation: u32,
}

impl MediaAsset {
    pub fn new(
        name: String,
        path: PathBuf,
        duration: Duration,
        width: u32,
        height: u32,
        fps: f64,
        has_audio: bool,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            path,
            duration,
            width,
            height,
            fps,
            has_audio,
            rotation: 0,
        }
    }

    /// Width after applying rotation (swaps for 90/270).
    pub fn display_width(&self) -> u32 {
        if self.rotation == 90 || self.rotation == 270 {
            self.height
        } else {
            self.width
        }
    }

    /// Height after applying rotation (swaps for 90/270).
    pub fn display_height(&self) -> u32 {
        if self.rotation == 90 || self.rotation == 270 {
            self.width
        } else {
            self.height
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SourceLibrary {
    assets: Vec<MediaAsset>,
}

impl SourceLibrary {
    pub fn new() -> Self {
        Self { assets: Vec::new() }
    }

    pub fn import(&mut self, asset: MediaAsset) {
        self.assets.push(asset);
    }

    pub fn remove(&mut self, id: Uuid) -> Result<MediaAsset> {
        let idx = self
            .assets
            .iter()
            .position(|a| a.id == id)
            .ok_or(CoreError::AssetNotFound(id))?;
        Ok(self.assets.remove(idx))
    }

    pub fn get(&self, id: Uuid) -> Option<&MediaAsset> {
        self.assets.iter().find(|a| a.id == id)
    }

    pub fn assets(&self) -> &[MediaAsset] {
        &self.assets
    }

    pub fn len(&self) -> usize {
        self.assets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }
}
