use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Represents a tool use content item
#[derive(Debug, Deserialize)]
struct ToolUse {
    #[serde(rename = "type")]
    type_: Option<String>,
    name: Option<String>,
    input: Option<Value>,
}

/// Represents the message content
#[derive(Debug, Deserialize)]
struct MessageContent {
    content: Option<Vec<ToolUse>>,
}

/// Represents a transcript line with message
#[derive(Debug, Deserialize)]
struct TranscriptLine {
    message: Option<MessageContent>,
}

/// A tool use event extracted from the transcript
#[derive(Debug, Clone)]
pub struct ToolUseEvent {
    pub tool_name: String,
    pub command: Option<String>,
    pub file_path: Option<String>,
    pub index: usize,
}

/// Extract all tool use events from a transcript file, in order
pub fn extract_tool_events(transcript_path: &str) -> Result<Vec<ToolUseEvent>> {
    let path = Path::new(transcript_path);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    let mut index = 0;

    for line in reader.lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }

        if let Ok(entry) = serde_json::from_str::<TranscriptLine>(&line) {
            if let Some(message) = entry.message {
                if let Some(content) = message.content {
                    for item in content {
                        if item.type_.as_deref() == Some("tool_use") {
                            if let Some(name) = &item.name {
                                let mut event = ToolUseEvent {
                                    tool_name: name.clone(),
                                    command: None,
                                    file_path: None,
                                    index,
                                };

                                // Extract relevant fields from input based on tool type
                                if let Some(input) = &item.input {
                                    match name.as_str() {
                                        "Bash" => {
                                            event.command = input
                                                .get("command")
                                                .and_then(|v| v.as_str())
                                                .map(String::from);
                                        }
                                        "Edit" | "Write" => {
                                            event.file_path = input
                                                .get("file_path")
                                                .and_then(|v| v.as_str())
                                                .map(String::from);
                                        }
                                        _ => {}
                                    }
                                }

                                events.push(event);
                                index += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nonexistent_transcript() {
        let events = extract_tool_events("/nonexistent/path.jsonl").unwrap();
        assert!(events.is_empty());
    }
}
