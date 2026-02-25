// LogSleuth - util/error.rs
//
// Typed error hierarchy with context-preserving error chains.
// No string-based error propagation (DevWorkflow Part A Rule 2).
// All errors preserve the causal chain for diagnostic logging.

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Top-level error type for all LogSleuth operations.
/// Errors are categorised by the subsystem that produced them.
#[derive(Debug)]
pub enum LogSleuthError {
    /// Profile loading or validation failed.
    Profile(ProfileError),

    /// File discovery failed.
    Discovery(DiscoveryError),

    /// Log file parsing failed.
    Parse(ParseError),

    /// Filter operation failed.
    Filter(FilterError),

    /// Export operation failed.
    Export(ExportError),

    /// Configuration loading or validation failed.
    Config(ConfigError),

    /// I/O error with path context.
    Io {
        path: PathBuf,
        operation: &'static str,
        source: io::Error,
    },
}

impl fmt::Display for LogSleuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Profile(e) => write!(f, "Profile error: {e}"),
            Self::Discovery(e) => write!(f, "Discovery error: {e}"),
            Self::Parse(e) => write!(f, "Parse error: {e}"),
            Self::Filter(e) => write!(f, "Filter error: {e}"),
            Self::Export(e) => write!(f, "Export error: {e}"),
            Self::Config(e) => write!(f, "Configuration error: {e}"),
            Self::Io {
                path,
                operation,
                source,
            } => write!(
                f,
                "I/O error during {operation} on '{}': {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for LogSleuthError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Profile(e) => Some(e),
            Self::Discovery(e) => Some(e),
            Self::Parse(e) => Some(e),
            Self::Filter(e) => Some(e),
            Self::Export(e) => Some(e),
            Self::Config(e) => Some(e),
            Self::Io { source, .. } => Some(source),
        }
    }
}

// ---------------------------------------------------------------------------
// Profile errors
// ---------------------------------------------------------------------------

/// Errors related to format profile loading and validation.
#[derive(Debug)]
pub enum ProfileError {
    /// TOML file could not be parsed.
    TomlParse {
        path: PathBuf,
        source: toml::de::Error,
    },

    /// Profile file exceeds the maximum allowed size.
    FileTooLarge {
        path: PathBuf,
        size: u64,
        max_size: u64,
    },

    /// A required field is missing from the profile definition.
    MissingField {
        profile_id: String,
        field: &'static str,
    },

    /// A regex pattern in the profile is invalid.
    InvalidRegex {
        profile_id: String,
        field: &'static str,
        pattern: String,
        source: regex::Error,
    },

    /// A regex pattern exceeds the maximum allowed length.
    RegexTooLong {
        profile_id: String,
        field: &'static str,
        length: usize,
        max_length: usize,
    },

    /// A timestamp format string is invalid.
    InvalidTimestampFormat {
        profile_id: String,
        format: String,
        reason: String,
    },

    /// Duplicate profile ID detected (user profile overriding built-in is OK,
    /// but two user profiles with the same ID is an error).
    DuplicateId {
        id: String,
        path1: PathBuf,
        path2: PathBuf,
    },

    /// Maximum number of profiles exceeded.
    TooManyProfiles { count: usize, max: usize },

    /// I/O error reading a profile file.
    Io { path: PathBuf, source: io::Error },
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TomlParse { path, source } => {
                write!(f, "Failed to parse TOML '{}': {source}", path.display())
            }
            Self::FileTooLarge {
                path,
                size,
                max_size,
            } => write!(
                f,
                "Profile '{}' is {size} bytes, exceeds maximum of {max_size} bytes",
                path.display()
            ),
            Self::MissingField { profile_id, field } => {
                write!(
                    f,
                    "Profile '{profile_id}': missing required field '{field}'"
                )
            }
            Self::InvalidRegex {
                profile_id,
                field,
                pattern,
                source,
            } => write!(
                f,
                "Profile '{profile_id}': invalid regex in '{field}' ('{pattern}'): {source}"
            ),
            Self::RegexTooLong {
                profile_id,
                field,
                length,
                max_length,
            } => write!(
                f,
                "Profile '{profile_id}': regex in '{field}' is {length} chars, \
                 exceeds maximum of {max_length}"
            ),
            Self::InvalidTimestampFormat {
                profile_id,
                format,
                reason,
            } => write!(
                f,
                "Profile '{profile_id}': invalid timestamp format '{format}': {reason}"
            ),
            Self::DuplicateId { id, path1, path2 } => write!(
                f,
                "Duplicate profile ID '{id}' in '{}' and '{}'",
                path1.display(),
                path2.display()
            ),
            Self::TooManyProfiles { count, max } => {
                write!(f, "Too many profiles loaded ({count}), maximum is {max}")
            }
            Self::Io { path, source } => {
                write!(
                    f,
                    "I/O error reading profile '{}': {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ProfileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::TomlParse { source, .. } => Some(source),
            Self::InvalidRegex { source, .. } => Some(source),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<ProfileError> for LogSleuthError {
    fn from(e: ProfileError) -> Self {
        Self::Profile(e)
    }
}

// ---------------------------------------------------------------------------
// Discovery errors
// ---------------------------------------------------------------------------

/// Errors related to file discovery.
#[derive(Debug)]
pub enum DiscoveryError {
    /// The root scan path does not exist or is not accessible.
    RootNotFound { path: PathBuf },

    /// The root path is not a directory.
    NotADirectory { path: PathBuf },

    /// Permission denied accessing the root path.
    PermissionDenied { path: PathBuf, source: io::Error },

    /// Maximum file count exceeded during scan.
    MaxFilesExceeded { max: usize },

    /// Walkdir traversal error (wraps individual file/dir access failures).
    Traversal {
        path: PathBuf,
        source: walkdir::Error,
    },
}

impl fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RootNotFound { path } => {
                write!(f, "Scan path '{}' does not exist", path.display())
            }
            Self::NotADirectory { path } => {
                write!(f, "Scan path '{}' is not a directory", path.display())
            }
            Self::PermissionDenied { path, source } => {
                write!(
                    f,
                    "Permission denied accessing '{}': {source}",
                    path.display()
                )
            }
            Self::MaxFilesExceeded { max } => {
                write!(
                    f,
                    "Discovery stopped: exceeded maximum of {max} files. \
                     Increase [discovery] max_files in config or narrow scan path."
                )
            }
            Self::Traversal { path, source } => {
                write!(f, "Error traversing '{}': {source}", path.display())
            }
        }
    }
}

impl std::error::Error for DiscoveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PermissionDenied { source, .. } => Some(source),
            Self::Traversal { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<DiscoveryError> for LogSleuthError {
    fn from(e: DiscoveryError) -> Self {
        Self::Discovery(e)
    }
}

// ---------------------------------------------------------------------------
// Parse errors
// ---------------------------------------------------------------------------

/// Errors related to log file parsing.
#[derive(Debug)]
pub enum ParseError {
    /// A line in a log file could not be parsed.
    LineParse {
        file: PathBuf,
        line_number: u64,
        reason: String,
    },

    /// A timestamp string could not be parsed.
    TimestampParse {
        file: PathBuf,
        line_number: u64,
        raw_timestamp: String,
        format: String,
    },

    /// File encoding is not valid UTF-8.
    InvalidEncoding {
        file: PathBuf,
        source: std::string::FromUtf8Error,
    },

    /// I/O error while reading a log file.
    Io { file: PathBuf, source: io::Error },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LineParse {
                file,
                line_number,
                reason,
            } => write!(f, "'{}' line {line_number}: {reason}", file.display()),
            Self::TimestampParse {
                file,
                line_number,
                raw_timestamp,
                format,
            } => write!(
                f,
                "'{}' line {line_number}: cannot parse timestamp \
                 '{raw_timestamp}' with format '{format}'",
                file.display()
            ),
            Self::InvalidEncoding { file, source } => {
                write!(f, "'{}': invalid UTF-8 encoding: {source}", file.display())
            }
            Self::Io { file, source } => {
                write!(f, "'{}': I/O error: {source}", file.display())
            }
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidEncoding { source, .. } => Some(source),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<ParseError> for LogSleuthError {
    fn from(e: ParseError) -> Self {
        Self::Parse(e)
    }
}

// ---------------------------------------------------------------------------
// Filter errors
// ---------------------------------------------------------------------------

/// Errors related to filter operations.
#[derive(Debug)]
pub enum FilterError {
    /// User-provided regex is invalid.
    InvalidRegex {
        pattern: String,
        source: regex::Error,
    },
}

impl fmt::Display for FilterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRegex { pattern, source } => {
                write!(f, "Invalid filter regex '{pattern}': {source}")
            }
        }
    }
}

impl std::error::Error for FilterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidRegex { source, .. } => Some(source),
        }
    }
}

impl From<FilterError> for LogSleuthError {
    fn from(e: FilterError) -> Self {
        Self::Filter(e)
    }
}

// ---------------------------------------------------------------------------
// Export errors
// ---------------------------------------------------------------------------

/// Errors related to export operations.
#[derive(Debug)]
pub enum ExportError {
    /// I/O error writing the export file.
    Io { path: PathBuf, source: io::Error },

    /// CSV serialisation error.
    Csv { path: PathBuf, source: csv::Error },

    /// JSON serialisation error.
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },

    /// Export would exceed maximum entry count.
    TooManyEntries { count: usize, max: usize },
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "Export I/O error '{}': {source}", path.display())
            }
            Self::Csv { path, source } => {
                write!(f, "CSV export error '{}': {source}", path.display())
            }
            Self::Json { path, source } => {
                write!(f, "JSON export error '{}': {source}", path.display())
            }
            Self::TooManyEntries { count, max } => write!(
                f,
                "Export of {count} entries exceeds maximum of {max}. \
                 Apply filters to reduce the result set."
            ),
        }
    }
}

impl std::error::Error for ExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Csv { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<ExportError> for LogSleuthError {
    fn from(e: ExportError) -> Self {
        Self::Export(e)
    }
}

// ---------------------------------------------------------------------------
// Config errors
// ---------------------------------------------------------------------------

/// Errors related to configuration loading.
#[derive(Debug)]
pub enum ConfigError {
    /// TOML parsing failed.
    TomlParse {
        path: PathBuf,
        source: toml::de::Error,
    },

    /// A config value is out of the allowed range.
    ValueOutOfRange {
        field: String,
        value: String,
        expected: String,
    },

    /// I/O error reading config file.
    Io { path: PathBuf, source: io::Error },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TomlParse { path, source } => {
                write!(f, "Config parse error '{}': {source}", path.display())
            }
            Self::ValueOutOfRange {
                field,
                value,
                expected,
            } => write!(
                f,
                "Config '{field}' = '{value}' is out of range. Expected: {expected}"
            ),
            Self::Io { path, source } => {
                write!(f, "Config I/O error '{}': {source}", path.display())
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::TomlParse { source, .. } => Some(source),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<ConfigError> for LogSleuthError {
    fn from(e: ConfigError) -> Self {
        Self::Config(e)
    }
}

/// Convenience type alias for LogSleuth results.
pub type Result<T> = std::result::Result<T, LogSleuthError>;
