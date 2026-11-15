//! Tauri commands for Roster's account layer.
//!
//! Thin wrappers over `account_manager` plus Donut's profile launcher. The heavy
//! engines (refresh over CDP, encrypted export/import, browser import) land in
//! later iterations; this file is the first verified slice: list accounts, open
//! one signed-in, and edit an account's tags/note.

use crate::account_manager::{
  open_account_url, parse_account_info, remap_imported_accounts, AccountManager, GoogleAccount,
};
use crate::profile::ProfileManager;
use crate::roster_pack::{pack, unpack, ProfileBlob, RosterBundle};

/// Read the accounts a profile currently holds from its on-disk Chromium
/// `Preferences`. Returns an empty list (not an error) when the profile has
/// never launched, so a freshly created slot refreshes to "no accounts yet".
fn read_profile_preferences(profile_id: &str) -> Vec<crate::account_manager::ListAccountEntry> {
  let data_dir = ProfileManager::instance()
    .get_profiles_dir()
    .join(profile_id)
    .join("profile");
  let prefs = {
    let with_default = data_dir.join("Default").join("Preferences");
    if with_default.exists() {
      with_default
    } else {
      data_dir.join("Preferences")
    }
  };
  match std::fs::read_to_string(&prefs) {
    Ok(body) => parse_account_info(&body),
    Err(_) => Vec::new(),
  }
}

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

/// Re-read one profile's accounts from disk and reconcile them into the store.
/// Returns that profile's accounts after the refresh.
#[tauri::command]
pub async fn refresh_profile(profile_id: String) -> Result<Vec<GoogleAccount>, String> {
  let entries = read_profile_preferences(&profile_id);
  AccountManager::instance().apply_profile_refresh(
    &profile_id,
    &entries,
    crate::proxy_manager::now_secs(),
  )
}

/// Refresh every profile Roster tracks accounts for, and return all accounts.
/// Offline file reads, so there is no need to bound concurrency here; the
/// bounded, session-warming launch refresh is a separate path.
#[tauri::command]
pub async fn refresh_all_accounts() -> Result<Vec<GoogleAccount>, String> {
  let manager = AccountManager::instance();
  let now = crate::proxy_manager::now_secs();
  for (profile_id, _count) in manager.usage_by_profile()? {
    let entries = read_profile_preferences(&profile_id);
    manager.apply_profile_refresh(&profile_id, &entries, now)?;
  }
  manager.list()
}

/// Export the selected accounts and the slot profiles holding them to one
/// password-protected `.roster` file. Returns how many accounts were written.
///
/// Each distinct profile is exported with Donut's portable export (session data
/// included), then everything is sealed with `roster_pack`. Import is additive,
/// so moving to another machine brings these accounts in alongside whatever is
/// there; to keep a session off two machines at once, stop using the exported
/// accounts here after the move.
#[tauri::command]
pub async fn export_accounts(
  account_ids: Vec<String>,
  destination: String,
  password: String,
) -> Result<usize, String> {
  if password.is_empty() {
    return Err("PASSWORD_REQUIRED".to_string());
  }
  let selected: Vec<GoogleAccount> = AccountManager::instance()
    .list()?
    .into_iter()
    .filter(|a| account_ids.contains(&a.id))
    .collect();
  if selected.is_empty() {
    return Err("NO_ACCOUNTS_SELECTED".to_string());
  }

  let mut profile_ids: Vec<String> = selected.iter().map(|a| a.profile_id.clone()).collect();
  profile_ids.sort();
  profile_ids.dedup();

  // A profile archive holds live session cookies, so it must never land in the
  // world-readable shared temp dir under a predictable name. A private TempDir
  // is 0700 on Unix and drops (deleting its contents) on every exit path.
  let workdir = tempfile::Builder::new()
    .prefix("roster-export-")
    .tempdir()
    .map_err(|e| e.to_string())?;

  let mut profiles = Vec::new();
  for pid in &profile_ids {
    let tmp = workdir.path().join(format!("{pid}.donutprofile"));
    let tmp_str = tmp.to_string_lossy().to_string();
    let manifest =
      crate::profile::portable::export_profile(pid.clone(), tmp_str, Some(true)).await?;
    let archive = std::fs::read(&tmp).map_err(|e| e.to_string())?;
    profiles.push(ProfileBlob {
      profile_id: pid.clone(),
      name: manifest.profile_name,
      archive,
    });
  }

  let bundle = RosterBundle::new(selected, profiles);
  let count = bundle.accounts.len();
  let file = pack(&bundle, &password)?;
  std::fs::write(&destination, file).map_err(|e| e.to_string())?;
  Ok(count)
}

/// Import accounts and their profiles from a `.roster` file. The password is
/// verified (and the file decrypted whole) before anything is written, so a
/// wrong password or a corrupt file changes nothing. Additive: imported
/// profiles are created fresh and the accounts repointed at them. Returns how
/// many accounts were imported.
#[tauri::command]
pub async fn import_accounts(path: String, password: String) -> Result<usize, String> {
  let file = std::fs::read(&path).map_err(|e| e.to_string())?;
  // Fails here on a wrong password or tampering, before any profile is created.
  let bundle = unpack(&file, &password)?;

  // Session-bearing archives are staged in a private 0700 dir, not shared temp,
  // and the dir is removed on every exit path when it drops.
  let workdir = tempfile::Builder::new()
    .prefix("roster-import-")
    .tempdir()
    .map_err(|e| e.to_string())?;

  let mut id_map = std::collections::HashMap::new();
  for (index, blob) in bundle.profiles.iter().enumerate() {
    // The filename is an index, never `blob.profile_id`: that string comes from
    // the imported file and could carry `../` to escape the staging dir.
    let tmp = workdir.path().join(format!("{index}.donutprofile"));
    std::fs::write(&tmp, &blob.archive).map_err(|e| e.to_string())?;
    let new_profile = crate::profile::portable::import_profile_archive(
      tmp.to_string_lossy().to_string(),
      Some(blob.name.clone()),
    )
    .await?;
    id_map.insert(blob.profile_id.clone(), new_profile.id.to_string());
  }

  let accounts = remap_imported_accounts(&bundle.accounts, &id_map);
  let count = accounts.len();
  AccountManager::instance().add_accounts(accounts)?;
  Ok(count)
}
