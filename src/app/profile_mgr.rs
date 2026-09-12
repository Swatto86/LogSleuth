// LogSleuth - app/profile_mgr.rs
//
// Manages loading of format profiles from both built-in sources
// (embedded in the binary) and user-defined TOML files on disk.
// User profiles override built-in profiles with the same ID.

use crate::core::model::FormatProfile;
use crate::core::profile;
use crate::util::constants;
use crate::util::error::ProfileError;
use std::path::Path;

/// Load all available profiles: built-in first, then user-defined overrides.
///
/// User profiles with the same ID as a built-in profile replace the built-in.
/// Invalid profiles are logged and skipped (non-fatal).
///
/// Returns the merged list and any non-fatal errors encountered.
pub fn load_all_profiles(
    user_profile_dir: Option<&Path>,
) -> (Vec<FormatProfile>, Vec<ProfileError>) {
    let mut profiles = profile::load_builtin_profiles();
    let mut errors = Vec::new();

    // On Windows, register the built-in EVTX profile for Event Viewer files.
    #[cfg(target_os = "windows")]
    {
        let evtx_profile = profile::create_evtx_profile();
        tracing::info!(profile_id = %evtx_profile.id, "Registered Windows Event Log (.evtx) profile");
        profiles.push(evtx_profile);
    }

    tracing::info!(builtin_count = profiles.len(), "Loaded built-in profiles");

    // Load user-defined profiles if the directory exists
    if let Some(dir) = user_profile_dir {
        if dir.is_dir() {
            let (user_profiles, user_errors) = load_user_profiles(dir);
            errors.extend(user_errors);

            // Override built-in profiles with matching user profiles
            for user_profile in user_profiles {
                if let Some(pos) = profiles.iter().position(|p| p.id == user_profile.id) {
                    tracing::info!(
                        profile_id = %user_profile.id,
                        "User profile overrides built-in"
                    );
                    profiles[pos] = user_profile;
                } else {
                    tracing::info!(
                        profile_id = %user_profile.id,
                        "Loaded user-defined profile"
                    );
                    profiles.push(user_profile);
                }
            }
        } else {
            tracing::debug!(
                dir = %dir.display(),
                "User profile directory does not exist (skipping)"
            );
        }
    }

    // Enforce maximum profile count
    if profiles.len() > constants::MAX_PROFILES {
        tracing::warn!(
            count = profiles.len(),
            max = constants::MAX_PROFILES,
            "Too many profiles loaded, truncating"
        );
        errors.push(ProfileError::TooManyProfiles {
            count: profiles.len(),
            max: constants::MAX_PROFILES,
        });
        profiles.truncate(constants::MAX_PROFILES);
    }

    tracing::info!(total = profiles.len(), "Profile loading complete");

    (profiles, errors)
}

/// Load user-defined profiles from a directory.
fn load_user_profiles(dir: &Path) -> (Vec<FormatProfile>, Vec<ProfileError>) {
    let mut profiles = Vec::new();
    let mut errors = Vec::new();

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            errors.push(ProfileError::Io {
                path: dir.to_path_buf(),
                source: e,
            });
            return (profiles, errors);
        }
    };

    for entry_result in entries {
        let entry = match entry_result {
            Ok(e) => e,
            Err(e) => {
                errors.push(ProfileError::Io {
                    path: dir.to_path_buf(),
                    source: e,
                });
                continue;
            }
        };

        let path = entry.path();

        // Only process .toml files
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }

        let content = match read_profile(&path) {
            Ok(content) => content,
            Err(error) => {
                errors.push(error);
                continue;
            }
        };

        match profile::parse_profile_toml(&content, &path)
            .and_then(|def| profile::validate_and_compile(def, &path, false))
        {
            Ok(p) => profiles.push(p),
            Err(e) => errors.push(e),
        }
    }

    (profiles, errors)
}

/// Bound the read itself, including files that grow after their metadata is read.
fn read_profile(path: &Path) -> Result<String, ProfileError> {
    use std::io::Read;
    let io_error = |source| ProfileError::Io {
        path: path.to_owned(),
        source,
    };
    // Avoid opening known devices/FIFOs; inspect the opened handle again below.
    if !std::fs::metadata(path).map_err(io_error)?.is_file() {
        return Err(io_error(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "profile must be a regular file",
        )));
    }
    let file = std::fs::File::open(path).map_err(io_error)?;
    let metadata = file.metadata().map_err(io_error)?;
    if !metadata.is_file() {
        return Err(io_error(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "profile must be a regular file",
        )));
    }
    let limit = constants::MAX_PROFILE_FILE_SIZE;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 > limit {
        return Err(ProfileError::FileTooLarge {
            path: path.to_owned(),
            size: bytes.len() as u64,
            max_size: limit,
        });
    }
    String::from_utf8(bytes)
        .map_err(|e| io_error(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
}

/// Summarize partial profile loads with actionable rejected-file diagnostics.
pub fn profile_load_status(profiles: &[FormatProfile], errors: &[ProfileError]) -> String {
    let total = profiles.len();
    let external = profiles.iter().filter(|p| !p.is_builtin).count();
    if errors.is_empty() {
        format!("Profiles loaded - {total} total ({external} external).")
    } else {
        let details = errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "Profiles loaded with {} warning(s) - {total} total ({external} external).\n{details}",
            errors.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_reader_rejects_nonfiles_and_oversized_input() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_profile(dir.path()).is_err());
        let path = dir.path().join("profile.toml");
        std::fs::write(
            &path,
            vec![b'a'; constants::MAX_PROFILE_FILE_SIZE as usize + 1],
        )
        .unwrap();
        assert!(matches!(
            read_profile(&path),
            Err(ProfileError::FileTooLarge { .. })
        ));
        std::fs::write(&path, "valid UTF-8").unwrap();
        assert_eq!(read_profile(&path).unwrap(), "valid UTF-8");
    }

    #[test]
    fn profile_load_status_reports_rejected_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.toml");
        std::fs::write(&path, "[invalid").unwrap();
        let (profiles, errors) = load_all_profiles(Some(dir.path()));
        assert!(!profiles.is_empty(), "built-in profiles remain available");
        assert_eq!(errors.len(), 1);
        let status = profile_load_status(&profiles, &errors);
        assert!(status.contains("warning"));
        assert!(status.contains("broken.toml"));
        assert!(status.contains(&errors[0].to_string()));
        std::fs::remove_file(path).unwrap();
        let (profiles, errors) = load_all_profiles(Some(dir.path()));
        assert!(errors.is_empty());
        assert!(!profile_load_status(&profiles, &errors).contains("warning"));
    }
}
