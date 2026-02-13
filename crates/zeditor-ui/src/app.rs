use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Task};

use zeditor_core::project::Project;
use zeditor_core::timeline::{Clip, TimeRange, TimelinePosition};

use crate::message::Message;

pub struct App {
    pub project: Project,
    pub playback_position: TimelinePosition,
    pub is_playing: bool,
    pub status_message: String,
}

impl Default for App {
    fn default() -> Self {
        Self {
            project: Project::new("Untitled"),
            playback_position: TimelinePosition::zero(),
            is_playing: false,
            status_message: String::new(),
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ImportMedia(_path) => {
                self.status_message = "Importing...".into();
                // In a real app, we'd spawn a task to probe the file.
                // For now, this is handled by tests pushing MediaImported directly.
                Task::none()
            }
            Message::MediaImported(result) => {
                match result {
                    Ok(asset) => {
                        self.status_message = format!("Imported: {}", asset.name);
                        self.project.source_library.import(asset);
                    }
                    Err(e) => {
                        self.status_message = format!("Import failed: {e}");
                    }
                }
                Task::none()
            }
            Message::RemoveAsset(id) => {
                match self.project.source_library.remove(id) {
                    Ok(asset) => {
                        self.status_message = format!("Removed: {}", asset.name);
                    }
                    Err(e) => {
                        self.status_message = format!("Remove failed: {e}");
                    }
                }
                Task::none()
            }
            Message::AddClipToTimeline {
                asset_id,
                track_index,
                position,
            } => {
                if let Some(asset) = self.project.source_library.get(asset_id) {
                    let source_range = TimeRange {
                        start: TimelinePosition::zero(),
                        end: TimelinePosition::from_secs_f64(asset.duration.as_secs_f64()),
                    };
                    let clip = Clip::new(asset_id, position, source_range);
                    let result = self.project.command_history.execute(
                        &mut self.project.timeline,
                        "Add clip",
                        |tl| tl.add_clip(track_index, clip),
                    );
                    match result {
                        Ok(()) => {
                            self.status_message = "Clip added".into();
                        }
                        Err(e) => {
                            self.status_message = format!("Add clip failed: {e}");
                        }
                    }
                } else {
                    self.status_message = "Asset not found".into();
                }
                Task::none()
            }
            Message::MoveClip {
                source_track,
                clip_id,
                dest_track,
                position,
            } => {
                let result = self.project.command_history.execute(
                    &mut self.project.timeline,
                    "Move clip",
                    |tl| tl.move_clip(source_track, clip_id, dest_track, position),
                );
                if let Err(e) = result {
                    self.status_message = format!("Move failed: {e}");
                }
                Task::none()
            }
            Message::CutClip {
                track_index,
                position,
            } => {
                let result = self.project.command_history.execute(
                    &mut self.project.timeline,
                    "Cut clip",
                    |tl| tl.cut_at(track_index, position),
                );
                match result {
                    Ok(_) => {
                        self.status_message = "Clip cut".into();
                    }
                    Err(e) => {
                        self.status_message = format!("Cut failed: {e}");
                    }
                }
                Task::none()
            }
            Message::ResizeClip {
                track_index,
                clip_id,
                new_end,
            } => {
                let result = self.project.command_history.execute(
                    &mut self.project.timeline,
                    "Resize clip",
                    |tl| tl.resize_clip(track_index, clip_id, new_end),
                );
                if let Err(e) = result {
                    self.status_message = format!("Resize failed: {e}");
                }
                Task::none()
            }
            Message::Play => {
                self.is_playing = true;
                Task::none()
            }
            Message::Pause => {
                self.is_playing = false;
                Task::none()
            }
            Message::SeekTo(pos) => {
                self.playback_position = pos;
                Task::none()
            }
            Message::Undo => {
                match self
                    .project
                    .command_history
                    .undo(&mut self.project.timeline)
                {
                    Ok(()) => {
                        self.status_message = "Undone".into();
                    }
                    Err(e) => {
                        self.status_message = format!("Undo failed: {e}");
                    }
                }
                Task::none()
            }
            Message::Redo => {
                match self
                    .project
                    .command_history
                    .redo(&mut self.project.timeline)
                {
                    Ok(()) => {
                        self.status_message = "Redone".into();
                    }
                    Err(e) => {
                        self.status_message = format!("Redo failed: {e}");
                    }
                }
                Task::none()
            }
            Message::SaveProject | Message::LoadProject(_) => {
                // Placeholder for file dialog integration.
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let source_panel = self.view_source_library();
        let timeline_panel = self.view_timeline();
        let playback_panel = self.view_playback();

        let status = text(&self.status_message).size(14);

        let content = column![
            row![source_panel, playback_panel].spacing(10),
            timeline_panel,
            status,
        ]
        .spacing(10)
        .padding(10);

        container(content).into()
    }

    fn view_source_library(&self) -> Element<'_, Message> {
        let title = text("Source Library").size(18);

        let assets: Vec<Element<Message>> = self
            .project
            .source_library
            .assets()
            .iter()
            .map(|asset| {
                let label = text(&asset.name).size(14);
                let add_btn = button(text("Add to Timeline").size(12)).on_press(
                    Message::AddClipToTimeline {
                        asset_id: asset.id,
                        track_index: 0,
                        position: self.project.timeline.track(0).map_or(
                            TimelinePosition::zero(),
                            |t| t.end_position(),
                        ),
                    },
                );
                row![label, add_btn].spacing(5).into()
            })
            .collect();

        let import_btn = button(text("Import Media").size(14));
        let asset_list = scrollable(column(assets).spacing(4));

        column![title, import_btn, asset_list]
            .spacing(8)
            .width(300)
            .into()
    }

    fn view_timeline(&self) -> Element<'_, Message> {
        let title = text("Timeline").size(18);

        let tracks: Vec<Element<Message>> = self
            .project
            .timeline
            .tracks
            .iter()
            .enumerate()
            .map(|(i, track)| {
                let label = text(format!("Track {}: {} clips", i, track.clips.len())).size(14);
                let clip_list: Vec<Element<Message>> = track
                    .clips
                    .iter()
                    .map(|clip| {
                        let dur = clip.duration().as_secs_f64();
                        text(format!("{:.1}s", dur)).size(12).into()
                    })
                    .collect();

                row(
                    std::iter::once(label.into())
                        .chain(clip_list)
                        .collect::<Vec<_>>(),
                )
                .spacing(4)
                .into()
            })
            .collect();

        let undo_btn = button(text("Undo").size(12)).on_press(Message::Undo);
        let redo_btn = button(text("Redo").size(12)).on_press(Message::Redo);

        column![title, row![undo_btn, redo_btn].spacing(5), column(tracks).spacing(4)]
            .spacing(8)
            .into()
    }

    fn view_playback(&self) -> Element<'_, Message> {
        let title = text("Playback").size(18);

        let play_pause = if self.is_playing {
            button(text("Pause").size(14)).on_press(Message::Pause)
        } else {
            button(text("Play").size(14)).on_press(Message::Play)
        };

        let position = text(format!(
            "{:.1}s",
            self.playback_position.as_secs_f64()
        ))
        .size(14);

        column![title, play_pause, position].spacing(8).into()
    }
}
