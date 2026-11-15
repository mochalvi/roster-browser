//! Tauri commands for Roster's account layer.
//!
//! Thin wrappers over `account_manager` plus Donut's profile launcher. The heavy
//! engines (refresh over CDP, encrypted export/import, browser import) land in
//! later iterations; this file is the first verified slice: list accounts, open
//! one signed-in, and edit an account's tags/note.

use crate::account_manager::{open_account_url, AccountManager, GoogleAccount};
use crate::profile::ProfileManager;

/// Every Google account Roster tracks, for the account dashboard.
#[tauri::command]
pub async fn list_google_accounts() -> Result<Vec<GoogleAccount>, String> {
  AccountManager::instance().list()
}

/// Open a browser straight into `account_id`, signed in as that account.
///
/// Resolves the account's slot profile and launches it at
/// `google.com/?authuser=<index>`. If the profile is already running, Donut's
/// launcher opens the URL in a new tab of the same window instead of a second
/// process (one profile directory cannot be run twice).
#[tauri::command]
pub async fn open_google_account(
  app_handle: tauri::AppHandle,
  account_id: String,
) -> Result<(), String> {
  let account = AccountManager::instance()
    .list()?
    .into_iter()
    .find(|a| a.id == account_id)
    .ok_or("ACCOUNT_NOT_FOUND")?;

  let profile_uuid =
    uuid::Uuid::parse_str(&account.profile_id).map_err(|_| "PROFILE_NOT_FOUND".to_string())?;
  let profile = ProfileManager::instance()
    .lookup_profile(&profile_uuid)
    .map_err(|e| e.to_string())?
    .ok_or("PROFILE_NOT_FOUND")?;

  crate::browser_runner::launch_browser_profile(
    app_handle,
    profile,
    Some(open_account_url(account.authuser_index)),
    None,
  )
  .await
  .map(|_| ())
}

/// Edit an account's tags and/or note from the dashboard.
#[tauri::command]
pub async fn set_google_account_meta(
  account_id: String,
  tags: Option<Vec<String>>,
  note: Option<String>,
) -> Result<(), String> {
  AccountManager::instance().set_account_meta(&account_id, tags, note)
}
