use serde::Deserialize;

/// Input JSON from Claude Code hook system
#[derive(Debug, Deserialize)]
pub struct HookInput {
    pub hook_event_name: String,
    pub cwd: String,
    pub session_id: String,
    #[allow(dead_code)]
    pub transcript_path: String,
    /// Tool name (only present for PreToolUse/PostToolUse events)
    pub tool_name: Option<String>,
}
