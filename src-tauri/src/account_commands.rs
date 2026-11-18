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
use crate::profile::ProfileManager;
use crate::roster_pack::{pack, unpack, ProfileBlob, RosterBundle};

/// Google's own endpoint for "which accounts are signed in here", read with the
/// profile's cookies. `json=standard` returns the array `parse_list_accounts`
/// expects.
const LIST_ACCOUNTS_URL: &str =
  "https://accounts.google.com/ListAccounts?listPages=0&json=standard&source=ChromiumBrowser";

/// Ask Google which accounts a profile is signed into, by reading the profile's
/// cookies and calling `ListAccounts` directly.
///
/// Offline `Preferences` cannot see web logins (Wayfern disables Chrome's own
/// sign-in, so `account_info` stays empty), and driving the browser to fetch it
/// is blocked behind Wayfern's paid automation. So Roster decrypts the profile's
/// own Google cookies (Donut's `os_crypt`) and makes the same POST Chromium's
/// `GaiaAuthFetcher::StartListAccounts` makes — `Origin: www.google.com`, a
/// one-space form body to force a POST. No browser launch needed.
#[tauri::command]
pub async fn sync_profile_live(profile_id: String) -> Result<Vec<GoogleAccount>, String> {
  let uuid = uuid::Uuid::parse_str(&profile_id).map_err(|_| "PROFILE_NOT_FOUND".to_string())?;
  let profile = ProfileManager::instance()
    .lookup_profile(&uuid)
    .map_err(|e| e.to_string())?
    .ok_or("PROFILE_NOT_FOUND")?;

  // shortcut: a proxied/VPN profile's ListAccounts call must go out its proxy,
  // not the real IP; routing reqwest through the profile proxy is not wired yet,
  // so such profiles are skipped (never leaked). Upgrade: add proxy routing.
  if profile.proxy_id.is_some() || profile.vpn_id.is_some() {
    return Err("PROXY_ROUTING_NOT_SUPPORTED".to_string());
  }

  let entries = fetch_list_accounts(&profile_id).await;
  AccountManager::instance().apply_profile_refresh(
    &profile_id,
    &entries?,
    crate::proxy_manager::now_secs(),
  )
}

/// Live-Sync every direct (un-proxied) Wayfern profile and return all accounts.
/// A freshly created profile has no tracked accounts yet, so this iterates
/// Donut's profiles directly rather than the ones Roster already knows.
#[tauri::command]
pub async fn sync_all_accounts_live() -> Result<Vec<GoogleAccount>, String> {
  let profiles = ProfileManager::instance()
    .list_profiles()
    .map_err(|e| e.to_string())?;
  for profile in profiles.into_iter().filter(|p| p.browser == "wayfern") {
    // One profile failing (not logged in, proxied, no cookies) must not abort
    // the rest; its accounts simply stay as they were.
    let _ = sync_profile_live(profile.id.to_string()).await;
  }
  AccountManager::instance().list()
}

/// Keep sessions from going stale: open each direct Wayfern profile briefly at
/// a Google page (headless) so Chromium refreshes and persists its session
/// cookies, then close it. Returns how many profiles were refreshed.
///
/// shortcut: sequential, ~18s per profile, un-proxied profiles only (a proxied
/// launch is gated the same as Sync). Upgrade: schedule it and bound it.
#[tauri::command]
pub async fn keep_alive_all(app_handle: tauri::AppHandle) -> Result<usize, String> {
  let profiles = ProfileManager::instance()
    .list_profiles()
    .map_err(|e| e.to_string())?;
  let mut refreshed = 0usize;
  for profile in profiles.into_iter().filter(|p| p.browser == "wayfern") {
    if profile.proxy_id.is_some() || profile.vpn_id.is_some() {
      continue;
    }
    // Already open means already warm; leave it alone.
    if profile
      .process_id
      .is_some_and(crate::proxy_storage::is_process_running)
    {
      refreshed += 1;
      continue;
    }
    let launched = BrowserRunner::instance()
      .launch_browser_with_debugging(
        app_handle.clone(),
        &profile,
        Some("https://myaccount.google.com/".to_string()),
        None,
        true,
        &FingerprintGate::Enforce,
      )
      .await;
    if let Ok(running) = launched {
      // Let the page load so Chromium rotates and flushes the session cookies.
      tokio::time::sleep(Duration::from_secs(18)).await;
      let _ = crate::browser_runner::kill_browser_profile(app_handle.clone(), running).await;
      refreshed += 1;
    }
  }
  Ok(refreshed)
}

/// Decrypt the profile's Google cookies and POST ListAccounts, exactly as
/// Chromium does. Returns the signed-in accounts parsed from the response.
async fn fetch_list_accounts(profile_id: &str) -> Result<Vec<ListAccountEntry>, String> {
  let pid = profile_id.to_string();
  let result = tokio::task::spawn_blocking(move || {
    crate::cookie_manager::CookieManager::read_cookies(&pid)
  })
  .await
  .map_err(|e| e.to_string())??;

  // Build the Cookie header from google.com cookies, deduping by name (the same
  // auth cookie appears under several google subdomains). Header values must be
  // ASCII, so a stray binary value is skipped rather than breaking the request.
  let mut jar: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
  for domain in &result.domains {
    // Browser-style domain match: only genuine google.com hosts, so a lookalike
    // cookie (e.g. "google.com.evil.example") is never merged in and sent along.
    let host = domain.domain.trim_start_matches('.');
    if host != "google.com" && !host.ends_with(".google.com") {
      continue;
    }
    for cookie in &domain.cookies {
      if cookie.value.is_empty() || !cookie.name.is_ascii() || !cookie.value.is_ascii() {
        continue;
      }
      jar.insert(cookie.name.clone(), cookie.value.clone());
    }
  }
  if jar.is_empty() {
    return Err("NO_GOOGLE_COOKIES".to_string());
  }
  let cookie_header = jar
    .iter()
    .map(|(k, v)| format!("{k}={v}"))
    .collect::<Vec<_>>()
    .join("; ");

  let client = reqwest::Client::builder()
    .build()
    .map_err(|e| e.to_string())?;
  let resp = client
    .post(LIST_ACCOUNTS_URL)
    .header("Origin", "https://www.google.com")
    .header("Content-Type", "application/x-www-form-urlencoded")
    .header(
      "User-Agent",
      "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
       (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36",
    )
    .header("Cookie", cookie_header)
    .body(" ") // one space forces a POST, as Chromium does
    .send()
    .await
    .map_err(|e| format!("request failed: {e}"))?;

  let status = resp.status();
  let body = resp.text().await.map_err(|e| format!("read body: {e}"))?;
  if !status.is_success() {
    return Err(format!("ListAccounts HTTP {status}"));
  }
  Ok(parse_list_accounts(&body))
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
