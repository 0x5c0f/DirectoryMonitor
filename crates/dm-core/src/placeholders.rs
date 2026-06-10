/// Macro placeholder constants used in log formats, script arguments, etc.
pub const PLACEHOLDER_FILE: &str = "%file%";
pub const PLACEHOLDER_DIRECTORY: &str = "%directory%";
pub const PLACEHOLDER_EVENT: &str = "%event%";
pub const PLACEHOLDER_TIMESTAMP: &str = "%timestamp%";
pub const PLACEHOLDER_PATH: &str = "%path%";
pub const PLACEHOLDER_TARGET: &str = "%target%";
pub const PLACEHOLDER_USER: &str = "%user%";
pub const PLACEHOLDER_PROCESS: &str = "%process%";

/// All available placeholders for reference.
pub const ALL_PLACEHOLDERS: &[&str] = &[
    PLACEHOLDER_FILE,
    PLACEHOLDER_DIRECTORY,
    PLACEHOLDER_EVENT,
    PLACEHOLDER_TIMESTAMP,
    PLACEHOLDER_PATH,
    PLACEHOLDER_TARGET,
    PLACEHOLDER_USER,
    PLACEHOLDER_PROCESS,
];
