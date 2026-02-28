// LogSleuth - platform/unc_auth.rs
//
// Windows UNC network path authentication.
//
// Establishes authenticated SMB connections to UNC paths (\\server\share)
// by calling the Windows WNet API (WNetAddConnection2W /
// WNetCancelConnection2W) directly rather than spawning net.exe.  No
// subprocess is created so anti-virus software will not trigger on unexpected
// child processes.
//
// # Security notes
// - Passwords are passed as pointers to stack-allocated wide-string buffers.
//   They are never written to any file, log, or other persistent storage.
//   (Rule 12)
// - Connections are created with dwFlags = 0 (no CONNECT_UPDATE_PROFILE) so
//   they are NOT written to the Windows Credential Manager and do not survive
//   a reboot.
//
// # Platform support
// WNet is a Windows-only API.  On other platforms every public function is a
// no-op stub that returns UncAuthError::NotSupported so the rest of the code
// compiles unchanged.

use std::path::Path;

// =============================================================================
// Error type
// =============================================================================

/// Error produced by a UNC connect or disconnect operation.
#[derive(Debug, Clone)]
pub enum UncAuthError {
    /// The WNet API returned a non-zero error code.
    ApiError { code: u32, message: String },
    /// Feature not available on this platform.
    NotSupported,
}

impl std::fmt::Display for UncAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApiError { message, .. } if !message.trim().is_empty() => {
                write!(f, "{}", message.trim())
            }
            Self::ApiError { code, .. } => write!(f, "Network error (code {code})"),
            Self::NotSupported => write!(
                f,
                "Network credential authentication is only supported on Windows"
            ),
        }
    }
}

// =============================================================================
// Path helpers (cross-platform)
// =============================================================================

/// Returns `true` if `path` is a UNC path (`\\server\…` or `//server/…`).
pub fn is_unc_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.starts_with("\\\\") || s.starts_with("//")
}

/// Extract the share root portion (`\\server\share`) from a UNC path.
///
/// Returns `None` if the path is not a UNC path or contains fewer than two
/// path components after the leading `\\`.
///
/// # Examples
/// - `\\srv\c$\ProgramData` → `Some("\\\\srv\\c$")`
/// - `\\srv\share` → `Some("\\\\srv\\share")`
/// - `C:\logs` → `None`
pub fn unc_share_root(path: &Path) -> Option<String> {
    let s = path.to_string_lossy();
    // Strip the leading `\\` or `//` so we can split on separators.
    let after_prefix = s.strip_prefix("\\\\").or_else(|| s.strip_prefix("//"))?;

    // Split on both `\` and `/` to get [server, share, rest...]
    let parts: Vec<&str> = after_prefix.splitn(3, ['\\', '/']).collect();

    if parts.len() >= 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        Some(format!("\\\\{}\\{}", parts[0], parts[1]))
    } else {
        None
    }
}

// =============================================================================
// Windows FFI declarations
// =============================================================================

/// Windows-only FFI block containing the NETRESOURCEW struct and the two
/// WNet API functions we need.  Everything in this block is `unsafe` by
/// nature; callers must uphold the documented invariants.
#[cfg(target_os = "windows")]
mod ffi {
    /// NETRESOURCEW.dwType: request a disk resource (SMB share).
    pub const RESOURCETYPE_DISK: u32 = 0x0000_0001;

    /// dwFlags = 0: non-persistent connection, not written to credential store.
    pub const CONNECT_FLAGS_TEMPORARY: u32 = 0;

    /// The NETRESOURCEW structure passed to WNetAddConnection2W.
    ///
    /// All pointer fields that are unused must be null.  `lp_remote_name` is
    /// the only field we populate; the rest are zero-initialised.
    #[repr(C)]
    pub struct NetResourceW {
        pub dw_scope: u32,
        pub dw_type: u32,
        pub dw_display_type: u32,
        pub dw_usage: u32,
        /// Unused: we do not map to a local drive letter.
        pub lp_local_name: *mut u16,
        /// The UNC share root in wide-string form (`\\server\share\0`).
        pub lp_remote_name: *mut u16,
        pub lp_comment: *mut u16,
        pub lp_provider: *mut u16,
    }

    // SAFETY: raw pointers in NetResourceW are only ever passed to WNet API
    // functions that treat them as read-only during the call.  The struct is
    // never shared across threads.
    unsafe impl Send for NetResourceW {}

    #[link(name = "Mpr")]
    unsafe extern "system" {
        /// Add a network connection.
        ///
        /// Returns ERROR_SUCCESS (0) on success, or a Win32 error code.
        pub fn WNetAddConnection2W(
            lp_net_resource: *const NetResourceW,
            lp_password: *const u16,
            lp_user_name: *const u16,
            dw_flags: u32,
        ) -> u32;

        /// Cancel (remove) a network connection.
        ///
        /// Returns ERROR_SUCCESS (0) on success, or a Win32 error code.
        /// `f_force` = TRUE (1) forces disconnection even if files are open.
        pub fn WNetCancelConnection2W(lp_name: *const u16, dw_flags: u32, f_force: i32) -> u32;

        /// Retrieve a human-readable message for a Win32 error code.
        pub fn FormatMessageW(
            dw_flags: u32,
            lp_source: *const std::ffi::c_void,
            dw_message_id: u32,
            dw_language_id: u32,
            lp_buffer: *mut u16,
            n_size: u32,
            arguments: *mut std::ffi::c_void,
        ) -> u32;

        /// Free a buffer allocated by FormatMessageW with FORMAT_MESSAGE_ALLOCATE_BUFFER.
        pub fn LocalFree(h_mem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    }
}

// =============================================================================
// Windows error helper
// =============================================================================

/// Translate a Win32 error code to a human-readable string.
/// Uses well-known messages for the most common codes, then falls back to
/// `FormatMessageW` for anything else.
#[cfg(target_os = "windows")]
fn win32_error_message(code: u32) -> String {
    match code {
        0 => return "Success".to_string(),
        5 => return "Access denied. Check that the username and password are correct.".to_string(),
        53 => return "Network path not found. Verify the server name and share.".to_string(),
        86 => return "Invalid password.".to_string(),
        1219 => {
            return "A connection already exists under different credentials. \
                    Try disconnecting first."
                .to_string()
        }
        1326 => return "Logon failure: incorrect username or password.".to_string(),
        1327 => return "Account restriction prevents logon.".to_string(),
        1330 => return "Password has expired; please change it and try again.".to_string(),
        _ => {}
    }

    // FORMAT_MESSAGE_ALLOCATE_BUFFER | FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS
    const FLAGS: u32 = 0x0000_0100 | 0x0000_1000 | 0x0000_0200;

    let mut buf: *mut u16 = std::ptr::null_mut();
    let len = unsafe {
        ffi::FormatMessageW(
            FLAGS,
            std::ptr::null(),
            code,
            0,
            std::ptr::addr_of_mut!(buf) as *mut u16,
            0,
            std::ptr::null_mut(),
        )
    };

    if len == 0 || buf.is_null() {
        return format!("Error code {code}");
    }

    // SAFETY: FormatMessageW guarantees `buf` points to `len` valid wide chars.
    let msg = unsafe { String::from_utf16_lossy(std::slice::from_raw_parts(buf, len as usize)) };
    // SAFETY: We must free the buffer allocated by FormatMessageW.
    unsafe {
        ffi::LocalFree(buf.cast());
    }
    msg.trim().to_string()
}

// =============================================================================
// Connection operations — Windows implementation
// =============================================================================

/// Establish an authenticated SMB connection to `share` using the Windows
/// WNet API (WNetAddConnection2W).
///
/// `share` must be in `\\server\share` form — use [`unc_share_root`] to derive
/// it from a longer UNC path.
///
/// `username` may use any format accepted by Windows:
/// - `user@domain.co.uk` (UPN — recommended)
/// - `DOMAIN\user`
/// - just `user` for local accounts
///
/// # Security
/// The password is passed as a pointer to a stack-allocated wide-string
/// buffer.  It is never written to any file, log, process list, or persistent
/// store.  (Rule 12)
///
/// # Persistence
/// `dwFlags = 0` means the connection is **not** written to the Windows
/// Credential Manager and will not be re-established after reboot.
pub fn connect_unc(share: &str, username: &str, password: &str) -> Result<(), UncAuthError> {
    #[cfg(target_os = "windows")]
    {
        // Convert Rust strings to null-terminated UTF-16 wide strings.
        let share_w: Vec<u16> = share.encode_utf16().chain(std::iter::once(0)).collect();
        let user_w: Vec<u16> = username.encode_utf16().chain(std::iter::once(0)).collect();
        let pass_w: Vec<u16> = password.encode_utf16().chain(std::iter::once(0)).collect();

        // Pre-emptively disconnect any existing connection to this share.
        // Windows returns error 1219 when a prior connection exists under
        // different credentials; removing it first avoids that collision.
        // Ignore the return value — if there was nothing to disconnect, the
        // call is harmless.
        disconnect_unc_inner(&share_w);

        let net_res = ffi::NetResourceW {
            dw_scope: 0,
            dw_type: ffi::RESOURCETYPE_DISK,
            dw_display_type: 0,
            dw_usage: 0,
            lp_local_name: std::ptr::null_mut(),
            // SAFETY: share_w is live for the entire duration of the call.
            lp_remote_name: share_w.as_ptr() as *mut u16,
            lp_comment: std::ptr::null_mut(),
            lp_provider: std::ptr::null_mut(),
        };

        // SAFETY: all pointer fields point to stack/Vec allocations that
        //         outlive the WNetAddConnection2W call.
        let rc = unsafe {
            ffi::WNetAddConnection2W(
                std::ptr::addr_of!(net_res),
                pass_w.as_ptr(),
                user_w.as_ptr(),
                ffi::CONNECT_FLAGS_TEMPORARY,
            )
        };
        if rc == 0 {
            tracing::info!(share = %share, "UNC connection established via WNet API");
            Ok(())
        } else {
            let msg = win32_error_message(rc);
            tracing::warn!(
                share = %share,
                code = rc,
                // Username logged; password intentionally omitted (Rule 12).
                username = %username,
                "WNetAddConnection2W failed"
            );
            Err(UncAuthError::ApiError {
                code: rc,
                message: msg,
            })
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (share, username, password);
        Err(UncAuthError::NotSupported)
    }
}

/// Disconnect an existing network connection to `share`.
///
/// Best-effort: failures are logged at WARN level but never propagated so the
/// caller's cleanup path is never blocked by a stale connection.
pub fn disconnect_unc(share: &str) {
    #[cfg(target_os = "windows")]
    {
        let share_w: Vec<u16> = share.encode_utf16().chain(std::iter::once(0)).collect();
        let rc = disconnect_unc_inner(&share_w);
        if rc == 0 {
            tracing::info!(share = %share, "UNC connection disconnected");
        } else {
            tracing::warn!(
                share = %share,
                code = rc,
                "WNetCancelConnection2W failed (ignored)"
            );
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = share;
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Raw `WNetCancelConnection2W` call.  Accepts an already-encoded wide-string
/// slice so callers can reuse their buffer.  Returns the Win32 error code
/// (0 = success).
#[cfg(target_os = "windows")]
fn disconnect_unc_inner(share_w: &[u16]) -> u32 {
    // SAFETY: share_w is a null-terminated slice that outlives the call.
    //         f_force = 1 (TRUE) so we disconnect even if files are open.
    unsafe { ffi::WNetCancelConnection2W(share_w.as_ptr(), 0, 1) }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn unc_share_root_extracts_server_and_share() {
        let path = PathBuf::from(r"\\sabpveeam-21\c$\ProgramData\Veeam");
        assert_eq!(
            unc_share_root(&path),
            Some(r"\\sabpveeam-21\c$".to_string())
        );
    }

    #[test]
    fn unc_share_root_bare_share() {
        let path = PathBuf::from(r"\\server\logs");
        assert_eq!(unc_share_root(&path), Some(r"\\server\logs".to_string()));
    }

    #[test]
    fn unc_share_root_forward_slashes() {
        let path = PathBuf::from("//fileserver/share/dir");
        assert!(unc_share_root(&path).is_some());
    }

    #[test]
    fn unc_share_root_local_path_returns_none() {
        let path = PathBuf::from(r"C:\Windows\logs");
        assert_eq!(unc_share_root(&path), None);
    }

    #[test]
    fn unc_share_root_missing_share_returns_none() {
        // Only server, no share component.
        let path = PathBuf::from(r"\\server_only");
        assert_eq!(unc_share_root(&path), None);
    }

    #[test]
    fn is_unc_path_detects_backslash_prefix() {
        assert!(is_unc_path(&PathBuf::from(r"\\srv\share")));
    }

    #[test]
    fn is_unc_path_detects_forward_slash_prefix() {
        assert!(is_unc_path(&PathBuf::from("//srv/share")));
    }

    #[test]
    fn is_unc_path_rejects_local() {
        assert!(!is_unc_path(&PathBuf::from(r"C:\logs")));
    }
}
