use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::timeline::Timeline;

/// A command that can be applied to and reverted from a timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    /// Snapshot-based: stores the full timeline state before and after.
    Snapshot {
        description: String,
        before: Timeline,
        after: Timeline,
    },
}

impl Command {
    /// Create a snapshot command by capturing the timeline state before and after an operation.
    pub fn snapshot(
        description: impl Into<String>,
        before: &Timeline,
        after: &Timeline,
    ) -> Self {
        Command::Snapshot {
            description: description.into(),
            before: before.clone(),
            after: after.clone(),
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Command::Snapshot { description, .. } => description,
        }
    }
}

/// Undo/redo history using snapshot commands.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommandHistory {
    undo_stack: Vec<Command>,
    redo_stack: Vec<Command>,
}

impl CommandHistory {
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Execute an operation on the timeline, recording it for undo.
    /// The closure receives a mutable reference to the timeline.
    /// Returns the result of the closure.
    pub fn execute<F, T>(&mut self, timeline: &mut Timeline, description: &str, f: F) -> Result<T>
    where
        F: FnOnce(&mut Timeline) -> Result<T>,
    {
        let before = timeline.clone();
        let result = f(timeline)?;
        let cmd = Command::snapshot(description, &before, timeline);
        self.undo_stack.push(cmd);
        self.redo_stack.clear();
        Ok(result)
    }

    /// Undo the last command, restoring the timeline to its previous state.
    pub fn undo(&mut self, timeline: &mut Timeline) -> Result<()> {
        let cmd = self.undo_stack.pop().ok_or(CoreError::NothingToUndo)?;
        match &cmd {
            Command::Snapshot { before, .. } => {
                *timeline = before.clone();
            }
        }
        self.redo_stack.push(cmd);
        Ok(())
    }

    /// Redo the last undone command.
    pub fn redo(&mut self, timeline: &mut Timeline) -> Result<()> {
        let cmd = self.redo_stack.pop().ok_or(CoreError::NothingToRedo)?;
        match &cmd {
            Command::Snapshot { after, .. } => {
                *timeline = after.clone();
            }
        }
        self.undo_stack.push(cmd);
        Ok(())
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo_description(&self) -> Option<&str> {
        self.undo_stack.last().map(|c| c.description())
    }

    pub fn redo_description(&self) -> Option<&str> {
        self.redo_stack.last().map(|c| c.description())
    }
}
