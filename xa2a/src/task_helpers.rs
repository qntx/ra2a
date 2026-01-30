//! Task helper functions for the A2A SDK.
//!
//! Provides utility functions for creating and manipulating Task objects.

#![allow(dead_code)]

use uuid::Uuid;

use crate::types::{
    Artifact, Message, MessageSendParams, Part, Task, TaskArtifactUpdateEvent, TaskState,
    TaskStatus,
};

/// Generates a new UUID v4 string.
pub fn generate_id() -> String {
    Uuid::new_v4().to_string()
}

/// Generates a current timestamp in ISO 8601 format.
pub fn now_iso8601() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Creates a new task object from message send params.
///
/// Generates UUIDs for task and context IDs if they are not already present.
pub fn create_task_obj(params: &MessageSendParams) -> Task {
    let context_id = params
        .message
        .context_id
        .clone()
        .unwrap_or_else(generate_id);

    Task {
        id: generate_id(),
        context_id,
        status: TaskStatus::new(TaskState::Submitted),
        kind: "task".to_string(),
        history: Some(vec![params.message.clone()]),
        artifacts: None,
        metadata: None,
    }
}

/// Appends artifact data from an event to a task.
///
/// Handles creating the artifacts list if it doesn't exist, adding new artifacts,
/// and appending parts to existing artifacts based on the `append` flag.
pub fn append_artifact_to_task(task: &mut Task, event: &TaskArtifactUpdateEvent) {
    let artifacts = task.artifacts.get_or_insert_with(Vec::new);
    let artifact_id = &event.artifact.artifact_id;
    let append_parts = event.append.unwrap_or(false);

    let existing_idx = artifacts.iter().position(|a| &a.artifact_id == artifact_id);

    if !append_parts {
        // Replace or add new artifact
        if let Some(idx) = existing_idx {
            artifacts[idx] = event.artifact.clone();
        } else {
            artifacts.push(event.artifact.clone());
        }
    } else if let Some(idx) = existing_idx {
        // Append parts to existing artifact
        artifacts[idx].parts.extend(event.artifact.parts.clone());
    }
    // If append=true but artifact doesn't exist, we ignore (matching Python behavior)
}

/// Creates a text artifact with the given content.
pub fn build_text_artifact(text: impl Into<String>, artifact_id: impl Into<String>) -> Artifact {
    Artifact::new(artifact_id, vec![Part::text(text)])
}

/// Checks if server and client output modalities are compatible.
///
/// Returns true if:
/// - Client specifies no preferred output modes
/// - Server specifies no supported output modes
/// - There is at least one common modality
pub fn are_modalities_compatible(
    server_output_modes: Option<&[String]>,
    client_output_modes: Option<&[String]>,
) -> bool {
    match (client_output_modes, server_output_modes) {
        (None, _) | (Some(&[]), _) => true,
        (_, None) | (_, Some(&[])) => true,
        (Some(client), Some(server)) => client.iter().any(|c| server.contains(c)),
    }
}

/// Applies history length limit to a task.
///
/// If `history_length` is specified and the task has history,
/// truncates the history to the specified length from the end.
pub fn apply_history_length(mut task: Task, history_length: Option<i32>) -> Task {
    if let (Some(length), Some(history)) = (history_length, task.history.as_mut()) {
        let length = length.max(0) as usize;
        if history.len() > length {
            let start = history.len() - length;
            *history = history.split_off(start);
        }
    }
    task
}

/// Extracts text content from a message.
///
/// Joins all text parts with the specified delimiter.
pub fn get_message_text(message: &Message, delimiter: &str) -> String {
    message
        .parts
        .iter()
        .filter_map(|p| p.as_text())
        .collect::<Vec<_>>()
        .join(delimiter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_id() {
        let id1 = generate_id();
        let id2 = generate_id();
        assert_ne!(id1, id2);
        assert_eq!(id1.len(), 36); // UUID v4 format
    }

    #[test]
    fn test_now_iso8601() {
        let ts = now_iso8601();
        assert!(ts.contains('T'));
        assert!(ts.len() > 20);
    }

    #[test]
    fn test_are_modalities_compatible() {
        // Both None -> compatible
        assert!(are_modalities_compatible(None, None));

        // Client None -> compatible
        assert!(are_modalities_compatible(Some(&["text".to_string()]), None));

        // Common modality -> compatible
        let server = vec!["text".to_string(), "image".to_string()];
        let client = vec!["text".to_string()];
        assert!(are_modalities_compatible(Some(&server), Some(&client)));

        // No common modality -> incompatible
        let server = vec!["image".to_string()];
        let client = vec!["text".to_string()];
        assert!(!are_modalities_compatible(Some(&server), Some(&client)));
    }
}
