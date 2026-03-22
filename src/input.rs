use serde::Deserialize;

/// Input JSON from Claude Code hook system
#[derive(Debug, Deserialize)]
pub struct HookInput {
    pub hook_event_name: String,
    pub cwd: String,
    #[allow(dead_code)]
    pub session_id: String,
    pub transcript_path: String,
    /// Tool name (only present for PreToolUse/PostToolUse events)
    #[allow(dead_code)]
    pub tool_name: Option<String>,
}
