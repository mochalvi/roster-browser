//! Tauri commands for Roster's account layer.
//!
//! Thin wrappers over `account_manager` plus Donut's profile launcher. The heavy
//! engines (refresh over CDP, encrypted export/import, browser import) land in
//! later iterations; this file is the first verified slice: list accounts, open
//! one signed-in, and edit an account's tags/note.

use std::time::Duration;

use crate::account_manager::{
  open_account_url, parse_account_info, parse_list_accounts, remap_imported_accounts,
  AccountManager, GoogleAccount, ListAccountEntry,
};
use crate::browser_runner::BrowserRunner;
use crate::launch_gate::FingerprintGate;
use crate::profile::{BrowserProfile, ProfileManager};
use crate::roster_pack::{pack, unpack, ProfileBlob, RosterBundle};
use crate::wayfern_cdp::WayfernSession;

/// Google's own endpoint for "which accounts are signed in here", read with the
/// profile's cookies. `json=standard` returns the array `parse_list_accounts`
/// expects.
const LIST_ACCOUNTS_URL: &str =
  "https://accounts.google.com/ListAccounts?listPages=0&json=standard";

/// Ask Google, through the browser, which accounts a profile is signed into.
///
/// Offline `Preferences` cannot see web logins — Wayfern disables Chrome's own
/// sign-in, so `account_info` stays empty — so a real Sync launches the profile
/// (headless if it is not already open), reads `ListAccounts` inside it (which
/// authenticates with the profile's cookies and fingerprint, the way Google
/// expects), reconciles the result, and returns that profile's accounts. The
/// visit also keeps the session warm.
#[tauri::command]
pub async fn sync_profile_live(
  app_handle: tauri::AppHandle,
  profile_id: String,
) -> Result<Vec<GoogleAccount>, String> {
  let uuid = uuid::Uuid::parse_str(&profile_id).map_err(|_| "PROFILE_NOT_FOUND".to_string())?;
  let manager = ProfileManager::instance();
  let profile = manager
    .lookup_profile(&uuid)
    .map_err(|e| e.to_string())?
    .ok_or("PROFILE_NOT_FOUND")?;

  let we_launched = !profile
    .process_id
    .is_some_and(crate::proxy_storage::is_process_running);

  if we_launched {
    // Enforce, not Advisory: a Sync must never launch a profile whose proxy
    // fails to resolve or whose fingerprint mismatches, or the ListAccounts
    // request would leave over the real IP and flag the account. A blocked
    // profile errors out here and the caller skips it.
    BrowserRunner::instance()
      .launch_browser_with_debugging(
        app_handle.clone(),
        &profile,
        None,
        None,
        true,
        &FingerprintGate::Enforce,
      )
      .await
      .map_err(|e| e.to_string())?;
  }

  // Re-read: a launch fills in the process id and the CDP port.
  let profile = manager
    .lookup_profile(&uuid)
    .map_err(|e| e.to_string())?
    .ok_or("PROFILE_NOT_FOUND")?;

  let entries = read_list_accounts(&profile).await;

  // Stop the browser only if this Sync started it: an already-open window is
  // left alone, but a headless one we launched is not leaked.
  if we_launched {
    let _ = crate::browser_runner::kill_browser_profile(app_handle, profile).await;
  }

  let entries = entries?;
  AccountManager::instance().apply_profile_refresh(
    &profile_id,
    &entries,
    crate::proxy_manager::now_secs(),
  )
}

/// Live-Sync every Wayfern profile and return all accounts. A freshly created
/// profile has no tracked accounts yet, so this iterates Donut's profiles
/// directly rather than the ones Roster already knows.
///
/// shortcut: launches each profile in turn; fine for a handful, add a bound and
/// concurrency when fleets get large.
#[tauri::command]
pub async fn sync_all_accounts_live(
  app_handle: tauri::AppHandle,
) -> Result<Vec<GoogleAccount>, String> {
  let profiles = ProfileManager::instance()
    .list_profiles()
    .map_err(|e| e.to_string())?;
  for profile in profiles.into_iter().filter(|p| p.browser == "wayfern") {
    // One profile failing (not logged in, launch refused) must not abort the
    // rest; its accounts simply stay as they were.
    let _ = sync_profile_live(app_handle.clone(), profile.id.to_string()).await;
  }
  AccountManager::instance().list()
}

async fn read_list_accounts(profile: &BrowserProfile) -> Result<Vec<ListAccountEntry>, String> {
  let target = crate::cdp_target::resolve(profile)
    .await
    .map_err(|e| format!("{e:?}"))?;
  let mut session = WayfernSession::open(&target)
    .await
    .map_err(|e| format!("{e:?}"))?;
  let _ = session.call("Page.enable", serde_json::json!({})).await;
  let nav = session
    .call_then_await_navigation(
      "Page.navigate",
      serde_json::json!({ "url": LIST_ACCOUNTS_URL }),
      Duration::from_secs(25),
    )
    .await;
  let body = match nav {
    Ok(_) => {
      let reply = session
        .call(
          "Runtime.evaluate",
          serde_json::json!({ "expression": "document.body.innerText", "returnByValue": true }),
        )
        .await
        .map_err(|e| format!("{e:?}"));
      reply.map(|r| {
        r.get("result")
          .and_then(|v| v.get("value"))
          .and_then(|v| v.as_str())
          .unwrap_or("")
          .to_string()
      })
    }
    Err(e) => Err(format!("{e:?}")),
  };
  session.close().await;
  Ok(parse_list_accounts(&body?))
}

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
