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

        // Check file size
        let metadata = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                errors.push(ProfileError::Io {
                    path: path.clone(),
                    source: e,
                });
                continue;
            }
        };

        if metadata.len() > constants::MAX_PROFILE_FILE_SIZE {
            errors.push(ProfileError::FileTooLarge {
                path: path.clone(),
                size: metadata.len(),
                max_size: constants::MAX_PROFILE_FILE_SIZE,
            });
            continue;
        }

        // Read and parse the profile
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                errors.push(ProfileError::Io {
                    path: path.clone(),
                    source: e,
                });
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
