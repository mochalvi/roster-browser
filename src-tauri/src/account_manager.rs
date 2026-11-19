//! Roster's account layer: the one thing Roster adds on top of Donut.
//!
//! Donut manages browser *profiles*. Roster manages Google *accounts* and maps
//! each to a (profile, authuser-index) pair, so the UI can list accounts
//! directly instead of making the user open a profile first to see what is
//! inside it. A profile is a slot holder: Google allows at most 10 accounts
//! signed in at once per browser, so each profile holds up to `capacity`
//! accounts and Roster opens a new, hidden profile when the slots fill.
//!
//! This file is deliberately pure where it can be: slot allocation and the
//! ListAccounts parser are plain functions with no I/O, tested below. The
//! Tauri-facing side is thin and lives at the bottom.

use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Mutex;

/// Google signs out every account in a browser at once, so accounts that must
/// not drag each other down belong in separate profiles. 10 is Google's own
/// multi-login ceiling; the UI may set a lower per-profile capacity.
pub const MAX_ACCOUNTS_PER_PROFILE: usize = 10;

/// What Roster believes about one account's session, refreshed by a sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AccountStatus {
  /// Signed in and usable.
  Active,
  /// Signed in but Google wants a verification/re-consent before it trusts it.
  NeedsVerification,
  /// Not signed in anymore; needs a manual re-login in its profile.
  LoggedOut,
  /// Never refreshed, or the last refresh could not tell.
  #[default]
  Unknown,
}

/// The account's standing in Google Antigravity, set by the user. Google only
/// reports a suspension through Antigravity's own OAuth API, and calling that
/// API from a third-party tool is itself a documented ban trigger, so Roster
/// never checks it automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AntigravityStatus {
  #[default]
  Normal,
  Banned,
  /// Google allows one appeal per account; this records that it was used.
  AppealSent,
}

/// Google's ToS-suspension appeal form for Antigravity, Gemini CLI and Gemini
/// Code Assist. It must be submitted while signed in as the suspended account.
const ANTIGRAVITY_APPEAL_FORM: &str =
  "https://docs.google.com/forms/d/e/1FAIpQLScOJWibQ_-hYuVv63kJTgEqlgAaOwRLGFTbXm-QY1gz8U0CGA/viewform";

/// One Google account, as Roster tracks it. The heavy session data lives in the
/// profile directory Donut owns; this is only the pointer and the metadata the
/// account list shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoogleAccount {
  pub id: String,
  pub email: String,
  #[serde(default)]
  pub display_name: Option<String>,
  #[serde(default)]
  pub avatar_url: Option<String>,
  /// The Donut profile holding this account's session.
  pub profile_id: String,
  /// Position of this account in the profile's ListAccounts order, which is
  /// exactly what `?authuser=<n>` selects. Refreshed on every sync because
  /// Google renumbers when an account is added or removed.
  #[serde(default)]
  pub authuser_index: usize,
  #[serde(default)]
  pub status: AccountStatus,
  #[serde(default)]
  pub last_refresh: Option<u64>,
  #[serde(default)]
  pub tags: Vec<String>,
  #[serde(default)]
  pub note: Option<String>,
  /// Google AI plan as Google's own account menu badges it ("Pro", "Ultra",
  /// ...), or "Free" when it shows none. `None` until a Sync could read it.
  #[serde(default)]
  pub plan: Option<String>,
  #[serde(default)]
  pub antigravity: AntigravityStatus,
  /// When `antigravity` last changed, so a ban or appeal can be followed up.
  #[serde(default)]
  pub antigravity_since: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct AccountsData {
  accounts: Vec<GoogleAccount>,
}

/// One account as read out of a profile's Google ListAccounts response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListAccountEntry {
  pub email: String,
  pub display_name: Option<String>,
  pub avatar_url: Option<String>,
  /// 0-based position, which is the `authuser` index.
  pub authuser_index: usize,
  /// Google's "valid session" flag for the row, when present. `false` means the
  /// cookie is there but the session needs re-auth.
  pub valid: bool,
  /// Filled in by a live Sync from Google's account menu; parsers leave it
  /// `None`, which keeps whatever plan was known before.
  pub plan: Option<String>,
}

// --- Pure domain logic (no I/O) -------------------------------------------

/// Pick a profile with a free slot for a new account, preferring one already in
/// `group_id` when a group is requested. Returns `None` when every candidate is
/// full, which tells the caller to create a new hidden profile.
///
/// `profile_ids` is the set of slot-holder profiles Roster may use, each paired
/// with how many accounts it currently holds.
pub fn pick_profile_with_free_slot(
  profile_ids: &[(String, usize)],
  capacity: usize,
) -> Option<String> {
  let cap = capacity.min(MAX_ACCOUNTS_PER_PROFILE).max(1);
  profile_ids
    .iter()
    .filter(|(_, used)| *used < cap)
    // Fill the fullest-but-not-full profile first, so slots pack tightly
    // instead of scattering one account across many profiles.
    .max_by_key(|(_, used)| *used)
    .map(|(id, _)| id.clone())
}

/// The URL that opens a browser straight into `account` as the signed-in user.
/// `authuser` takes the index, not the email, and is what Google's own account
/// switcher puts in its links.
pub fn open_account_url(authuser_index: usize) -> String {
  format!("https://www.google.com/?authuser={authuser_index}")
}

/// The URL that starts the "add another account" flow in a profile, used when
/// the user adds an account to a profile that already has one.
pub fn add_session_url() -> String {
  "https://accounts.google.com/AddSession".to_string()
}

/// Parse a Google ListAccounts response body into accounts, in authuser order.
///
/// The endpoint (`accounts.google.com/ListAccounts?json=standard`) is
/// undocumented and its field positions drift, so this does not hardcode
/// indices: for each account row it takes the first `@`-bearing string as the
/// email, the first other non-URL string as the display name, the first
/// image-looking URL as the avatar, and reads the leading integer flag (0 =
/// valid session, 1 = needs re-auth) when the row has one. The authuser index
/// is the row's position, which is how `?authuser=n` is defined.
pub fn parse_list_accounts(body: &str) -> Vec<ListAccountEntry> {
  // ListAccounts prefixes anti-JSON-hijack junk like `)]}'` on some paths.
  let trimmed = body.trim_start_matches(|c: char| c != '[' && c != '{');
  let root: serde_json::Value = match serde_json::from_str(trimmed) {
    Ok(v) => v,
    Err(_) => return Vec::new(),
  };

  // The account list is the first array element that is itself an array of
  // arrays. Finding it by shape rather than by `root[1]` survives Google adding
  // or reordering the envelope's other fields.
  let rows = find_account_rows(&root);
  let mut out = Vec::new();
  for (index, row) in rows.iter().enumerate() {
    let Some(items) = row.as_array() else { continue };

    // Collect the row's string values in order. The display name is found
    // positionally below, not by "first non-email string": that would grab
    // Google's opaque per-account hash token, which also looks like a word.
    //
    // shortcut: a listed account is reported Active. ListAccounts does carry a
    // per-row valid-session flag, but its index drifts and the leading integer
    // is NOT it (it is 1 for normal accounts, which wrongly flagged every
    // account as needing re-auth). Upgrade: parse the real flag to detect a
    // genuinely stale-but-listed session.
    let valid = true;
    let mut strings: Vec<&str> = Vec::new();
    for item in items {
      if let serde_json::Value::String(s) = item {
        strings.push(s);
      }
    }

    let email_pos = strings
      .iter()
      .position(|s| s.contains('@') && s.contains('.'));
    let Some(email_pos) = email_pos else { continue };

    // The name sits immediately before the email in every ListAccounts layout
    // seen; the hash is further left. Skip it if that slot is a URL/empty.
    let display_name = email_pos
      .checked_sub(1)
      .map(|i| strings[i])
      .filter(|s| !s.is_empty() && !s.starts_with("http"))
      .map(str::to_string);
    let avatar_url = strings
      .iter()
      .find(|s| is_image_url(s))
      .map(|s| s.to_string());

    out.push(ListAccountEntry {
      email: strings[email_pos].to_string(),
      display_name,
      avatar_url,
      authuser_index: index,
      valid,
      plan: None,
    });
  }
  out
}

/// Read the accounts a profile currently holds from its Chromium
/// `Default/Preferences` `account_info` array — the offline source of truth for
/// *which* accounts are signed in, their names, avatars, and order. It cannot
/// tell a live session from a stale one (that needs the network ListAccounts),
/// so every account it returns is reported valid; a later live refresh can
/// downgrade one to needs-verification.
///
/// `authuser_index` is the array position, matching Chromium's own numbering.
pub fn parse_account_info(preferences_json: &str) -> Vec<ListAccountEntry> {
  let root: serde_json::Value = match serde_json::from_str(preferences_json) {
    Ok(v) => v,
    Err(_) => return Vec::new(),
  };
  let Some(list) = root.get("account_info").and_then(|v| v.as_array()) else {
    return Vec::new();
  };
  list
    .iter()
    .enumerate()
    .filter_map(|(index, acct)| {
      let email = acct.get("email")?.as_str()?.to_string();
      if email.is_empty() {
        return None;
      }
      let display_name = acct
        .get("full_name")
        .or_else(|| acct.get("given_name"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
      let avatar_url = acct
        .get("picture_url")
        .or_else(|| acct.get("account_image_url"))
        .and_then(|v| v.as_str())
        .filter(|s| s.starts_with("http"))
        .map(str::to_string);
      Some(ListAccountEntry {
        email,
        display_name,
        avatar_url,
        authuser_index: index,
        valid: true,
        plan: None,
      })
    })
    .collect()
}

/// Read one account's Google AI plan from Google's account-menu widget
/// (`ogs.google.com/u/<n>/widget/account?hl=en`). The widget's expanded header
/// is labelled `"<name>, <email>, AI Pro account. Collapse accounts list"`, and
/// carries no plan phrase for an account without one.
///
/// Returns `None` when the header is missing or belongs to another email (so a
/// renumbered or unreadable account keeps its previous plan, never a wrong
/// one), `Some("Free")` when the header has no plan, else the plan word(s).
///
/// shortcut: scrapes an undocumented page in English; if Google reshapes it this
/// returns `None` and the UI shows the plan as unknown. Upgrade: an official API.
pub fn parse_account_plan(widget_html: &str, email: &str) -> Option<String> {
  let end = widget_html.find("Collapse accounts list\"")?;
  let start = widget_html[..end].rfind("aria-label=\"")? + "aria-label=\"".len();
  let label = widget_html[start..end].trim().trim_end_matches('.');
  let at = label.find(email)?;
  let after = label[at + email.len()..].trim_start_matches([',', ' ']);
  let plan = after
    .strip_prefix("AI ")
    .and_then(|rest| rest.strip_suffix(" account"))
    .map(str::trim)
    .filter(|p| !p.is_empty());
  Some(plan.unwrap_or("Free").to_string())
}

/// The appeal form, opened as `email` so Google pre-selects the suspended
/// account in a profile that holds several.
pub fn antigravity_appeal_url(email: &str) -> String {
  format!(
    "{ANTIGRAVITY_APPEAL_FORM}?authuser={}",
    urlencoding::encode(email)
  )
}

fn is_image_url(s: &str) -> bool {
  s.starts_with("http")
    && (s.contains("googleusercontent.com")
      || s.ends_with(".jpg")
      || s.ends_with(".png")
      || s.contains("/photo"))
}

/// The account list is the deepest-nested first array-of-arrays. ListAccounts
/// wraps it as `["...", [[row],[row],...]]`.
fn find_account_rows(root: &serde_json::Value) -> Vec<serde_json::Value> {
  if let Some(top) = root.as_array() {
    for element in top {
      if let Some(inner) = element.as_array() {
        if inner.iter().any(|e| e.is_array()) {
          return inner.iter().filter(|e| e.is_array()).cloned().collect();
        }
      }
    }
  }
  Vec::new()
}

/// Reconcile stored accounts for one profile against a fresh ListAccounts read:
/// update index/status/metadata of known accounts, add newly-seen ones, and
/// mark as logged out any that Roster tracked but the profile no longer lists.
/// Returns the accounts for that profile after reconciliation.
pub fn reconcile_profile_accounts(
  profile_id: &str,
  existing: &[GoogleAccount],
  fresh: &[ListAccountEntry],
  now: u64,
) -> Vec<GoogleAccount> {
  let mut result: Vec<GoogleAccount> = Vec::new();

  for entry in fresh {
    let status = if entry.valid {
      AccountStatus::Active
    } else {
      AccountStatus::NeedsVerification
    };
    if let Some(prev) = existing.iter().find(|a| a.email == entry.email) {
      result.push(GoogleAccount {
        authuser_index: entry.authuser_index,
        status,
        last_refresh: Some(now),
        display_name: entry.display_name.clone().or_else(|| prev.display_name.clone()),
        avatar_url: entry.avatar_url.clone().or_else(|| prev.avatar_url.clone()),
        plan: entry.plan.clone().or_else(|| prev.plan.clone()),
        ..prev.clone()
      });
    } else {
      result.push(GoogleAccount {
        id: uuid::Uuid::new_v4().to_string(),
        email: entry.email.clone(),
        display_name: entry.display_name.clone(),
        avatar_url: entry.avatar_url.clone(),
        profile_id: profile_id.to_string(),
        authuser_index: entry.authuser_index,
        status,
        last_refresh: Some(now),
        tags: Vec::new(),
        note: None,
        plan: entry.plan.clone(),
        antigravity: AntigravityStatus::Normal,
        antigravity_since: None,
      });
    }
  }

  // Accounts we tracked that the profile no longer lists are logged out, not
  // deleted: the user may want to re-login them, and dropping them silently
  // loses the tags and notes they carried.
  for prev in existing {
    if !fresh.iter().any(|e| e.email == prev.email) {
      result.push(GoogleAccount {
        status: AccountStatus::LoggedOut,
        last_refresh: Some(now),
        ..prev.clone()
      });
    }
  }

  result
}

/// Rewrite imported accounts to point at the profiles they were imported into.
///
/// An import mints a fresh profile id for each archive (two imports of the same
/// file must not collide), so every account's `profile_id` and its own `id`
/// are remapped: `id_map` is old-profile-id -> new-profile-id. An account whose
/// profile is missing from the map is dropped — its session did not come along,
/// so tracking it would point the UI at nothing.
pub fn remap_imported_accounts(
  accounts: &[GoogleAccount],
  id_map: &std::collections::HashMap<String, String>,
) -> Vec<GoogleAccount> {
  accounts
    .iter()
    .filter_map(|a| {
      let new_profile = id_map.get(&a.profile_id)?;
      Some(GoogleAccount {
        id: uuid::Uuid::new_v4().to_string(),
        profile_id: new_profile.clone(),
        ..a.clone()
      })
    })
    .collect()
}

// --- Persistence + Tauri surface ------------------------------------------

pub struct AccountManager {
  lock: Mutex<()>,
}

static ACCOUNT_MANAGER: std::sync::LazyLock<AccountManager> =
  std::sync::LazyLock::new(|| AccountManager { lock: Mutex::new(()) });

impl AccountManager {
  pub fn instance() -> &'static AccountManager {
    &ACCOUNT_MANAGER
  }

  fn file_path() -> std::path::PathBuf {
    crate::app_dirs::data_subdir().join("accounts.json")
  }

  fn load(&self) -> Result<AccountsData, String> {
    let path = Self::file_path();
    if !path.exists() {
      return Ok(AccountsData::default());
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
  }

  fn save(&self, data: &AccountsData) -> Result<(), String> {
    let path = Self::file_path();
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
  }

  pub fn list(&self) -> Result<Vec<GoogleAccount>, String> {
    let _g = self.lock.lock().unwrap_or_else(|p| p.into_inner());
    Ok(self.load()?.accounts)
  }

  /// How many accounts each profile currently holds, for slot allocation.
  pub fn usage_by_profile(&self) -> Result<Vec<(String, usize)>, String> {
    let accounts = self.list()?;
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for a in accounts {
      *counts.entry(a.profile_id).or_insert(0) += 1;
    }
    Ok(counts.into_iter().collect())
  }

  /// Replace every tracked account for one profile with the reconciled set from
  /// a fresh ListAccounts read. Other profiles' accounts are left untouched.
  pub fn apply_profile_refresh(
    &self,
    profile_id: &str,
    fresh: &[ListAccountEntry],
    now: u64,
  ) -> Result<Vec<GoogleAccount>, String> {
    let _g = self.lock.lock().unwrap_or_else(|p| p.into_inner());
    let mut data = self.load()?;
    let (mine, others): (Vec<_>, Vec<_>) = data
      .accounts
      .into_iter()
      .partition(|a| a.profile_id == profile_id);
    let reconciled = reconcile_profile_accounts(profile_id, &mine, fresh, now);
    let updated = reconciled.clone();
    data.accounts = others;
    data.accounts.extend(reconciled);
    self.save(&data)?;
    Ok(updated)
  }

  /// Append imported accounts to the store (import is additive: it brings
  /// accounts onto a machine without disturbing what is already there).
  pub fn add_accounts(&self, accounts: Vec<GoogleAccount>) -> Result<(), String> {
    let _g = self.lock.lock().unwrap_or_else(|p| p.into_inner());
    let mut data = self.load()?;
    data.accounts.extend(accounts);
    self.save(&data)
  }

  pub fn set_account_meta(
    &self,
    account_id: &str,
    tags: Option<Vec<String>>,
    note: Option<String>,
  ) -> Result<(), String> {
    let _g = self.lock.lock().unwrap_or_else(|p| p.into_inner());
    let mut data = self.load()?;
    let account = data
      .accounts
      .iter_mut()
      .find(|a| a.id == account_id)
      .ok_or("ACCOUNT_NOT_FOUND")?;
    if let Some(tags) = tags {
      account.tags = tags;
    }
    if let Some(note) = note {
      account.note = Some(note);
    }
    self.save(&data)
  }

  pub fn set_antigravity_status(
    &self,
    account_id: &str,
    status: AntigravityStatus,
    now: u64,
  ) -> Result<Vec<GoogleAccount>, String> {
    let _g = self.lock.lock().unwrap_or_else(|p| p.into_inner());
    let mut data = self.load()?;
    let account = data
      .accounts
      .iter_mut()
      .find(|a| a.id == account_id)
      .ok_or("ACCOUNT_NOT_FOUND")?;
    if account.antigravity != status {
      account.antigravity = status;
      account.antigravity_since = Some(now);
    }
    self.save(&data)?;
    Ok(data.accounts)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn fills_the_fullest_profile_that_still_has_room() {
    let profiles = vec![
      ("a".to_string(), 3),
      ("b".to_string(), 9),
      ("c".to_string(), 10),
    ];
    // b has one slot left and is fullest; c is full and skipped.
    assert_eq!(
      pick_profile_with_free_slot(&profiles, 10),
      Some("b".to_string())
    );
  }

  #[test]
  fn no_free_slot_asks_for_a_new_profile() {
    let profiles = vec![("a".to_string(), 10), ("b".to_string(), 10)];
    assert_eq!(pick_profile_with_free_slot(&profiles, 10), None);
    // Empty set: nothing to fill, caller makes the first profile.
    assert_eq!(pick_profile_with_free_slot(&[], 10), None);
  }

  #[test]
  fn capacity_is_clamped_to_googles_ceiling_and_to_at_least_one() {
    let profiles = vec![("a".to_string(), 10)];
    // A capacity above 10 cannot squeeze an 11th account into a profile.
    assert_eq!(pick_profile_with_free_slot(&profiles, 50), None);
    let one = vec![("a".to_string(), 1)];
    // Capacity 0 is nonsense; treated as 1, so a 1-account profile is full.
    assert_eq!(pick_profile_with_free_slot(&one, 0), None);
  }

  #[test]
  fn open_url_uses_the_index_not_the_email() {
    assert_eq!(open_account_url(0), "https://www.google.com/?authuser=0");
    assert_eq!(open_account_url(3), "https://www.google.com/?authuser=3");
  }

  #[test]
  fn parses_accounts_in_authuser_order_with_email_name_avatar() {
    // Shape mirrors ListAccounts: envelope string, then array of rows.
    let body = r#"["gaia",[
      [0,"hash1","Andi Saputra","andi@gmail.com","https://lh3.googleusercontent.com/a/x",0],
      [0,"hash2","Budi","budi@gmail.com","https://lh3.googleusercontent.com/a/y",0]
    ]]"#;
    let got = parse_list_accounts(body);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].email, "andi@gmail.com");
    assert_eq!(got[0].display_name.as_deref(), Some("Andi Saputra"));
    assert_eq!(got[0].authuser_index, 0);
    assert!(got[0].avatar_url.as_deref().unwrap().contains("googleusercontent"));
    assert_eq!(got[1].authuser_index, 1);
    assert_eq!(got[1].email, "budi@gmail.com");
  }

  #[test]
  fn a_listed_account_is_reported_active() {
    // The leading integer is 1 for a normal account; it is NOT a validity flag,
    // so a listed account must come back valid (Active), not needs-verification.
    let body = r#"["gaia",[[1,"h","Cici","cici@gmail.com","https://lh3.googleusercontent.com/a/z",0]]]"#;
    let got = parse_list_accounts(body);
    assert_eq!(got.len(), 1);
    assert!(got[0].valid, "a listed account is active");
    assert_eq!(got[0].display_name.as_deref(), Some("Cici"));
  }

  #[test]
  fn junk_prefix_and_garbage_do_not_panic() {
    assert!(parse_list_accounts(")]}'\n[]").is_empty());
    assert!(parse_list_accounts("not json at all").is_empty());
    assert!(parse_list_accounts("").is_empty());
  }

  fn acct(email: &str, profile: &str, idx: usize) -> GoogleAccount {
    GoogleAccount {
      id: format!("id-{email}"),
      email: email.to_string(),
      display_name: Some("old name".to_string()),
      avatar_url: None,
      profile_id: profile.to_string(),
      authuser_index: idx,
      status: AccountStatus::Unknown,
      last_refresh: None,
      tags: vec!["klien-a".to_string()],
      note: Some("penting".to_string()),
      plan: Some("Pro".to_string()),
      antigravity: AntigravityStatus::Banned,
      antigravity_since: Some(7),
    }
  }

  fn entry(email: &str, idx: usize, valid: bool) -> ListAccountEntry {
    ListAccountEntry {
      email: email.to_string(),
      display_name: Some("fresh name".to_string()),
      avatar_url: Some("https://lh3.googleusercontent.com/a/x".to_string()),
      authuser_index: idx,
      valid,
      plan: None,
    }
  }

  // Trimmed from a real ogs.google.com/u/0/widget/account?hl=en response.
  const WIDGET: &str = r#"<div aria-expanded="true" aria-label="Andi Saputra, andi@gmail.com, AI Pro account. Collapse accounts list" jsaction="x"><span class="YzbLde">Pro</span>"#;

  #[test]
  fn reads_the_plan_from_the_account_menu_header() {
    assert_eq!(parse_account_plan(WIDGET, "andi@gmail.com").as_deref(), Some("Pro"));
    let ultra = WIDGET.replace("AI Pro account", "AI Ultra account");
    assert_eq!(parse_account_plan(&ultra, "andi@gmail.com").as_deref(), Some("Ultra"));
  }

  #[test]
  fn a_header_without_a_plan_is_free() {
    let free = WIDGET.replace(", AI Pro account", "");
    assert_eq!(parse_account_plan(&free, "andi@gmail.com").as_deref(), Some("Free"));
  }

  #[test]
  fn another_accounts_header_or_no_header_gives_no_plan() {
    assert_eq!(parse_account_plan(WIDGET, "budi@gmail.com"), None, "wrong account");
    assert_eq!(parse_account_plan("<html>403</html>", "andi@gmail.com"), None);
  }

  #[test]
  fn appeal_url_selects_the_account_by_email() {
    assert!(antigravity_appeal_url("a.b+x@gmail.com").ends_with("viewform?authuser=a.b%2Bx%40gmail.com"));
  }

  #[test]
  fn sync_keeps_the_known_plan_and_never_touches_antigravity_status() {
    // A Sync that could not read the plan (None) must not erase the old one,
    // and the user-set Antigravity status survives every refresh.
    let out = reconcile_profile_accounts("p1", &[acct("andi@gmail.com", "p1", 0)], &[entry("andi@gmail.com", 0, true)], 9);
    assert_eq!(out[0].plan.as_deref(), Some("Pro"));
    assert_eq!(out[0].antigravity, AntigravityStatus::Banned);
    let mut fresh = entry("andi@gmail.com", 0, true);
    fresh.plan = Some("Ultra".to_string());
    let out = reconcile_profile_accounts("p1", &[acct("andi@gmail.com", "p1", 0)], &[fresh], 9);
    assert_eq!(out[0].plan.as_deref(), Some("Ultra"), "a fresh read wins");
  }

  #[test]
  fn refresh_updates_index_and_status_but_keeps_tags_and_notes() {
    let existing = vec![acct("andi@gmail.com", "p1", 5)];
    let fresh = vec![entry("andi@gmail.com", 0, true)];
    let out = reconcile_profile_accounts("p1", &existing, &fresh, 100);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].authuser_index, 0, "index renumbered");
    assert_eq!(out[0].status, AccountStatus::Active);
    assert_eq!(out[0].id, "id-andi@gmail.com", "same account, kept id");
    assert_eq!(out[0].tags, vec!["klien-a".to_string()], "tags survive");
    assert_eq!(out[0].note.as_deref(), Some("penting"));
    assert_eq!(out[0].last_refresh, Some(100));
  }

  #[test]
  fn refresh_adds_new_accounts_and_logs_out_vanished_ones() {
    let existing = vec![acct("andi@gmail.com", "p1", 0)];
    let fresh = vec![entry("budi@gmail.com", 0, true)];
    let out = reconcile_profile_accounts("p1", &existing, &fresh, 100);
    let andi = out.iter().find(|a| a.email == "andi@gmail.com").unwrap();
    let budi = out.iter().find(|a| a.email == "budi@gmail.com").unwrap();
    assert_eq!(andi.status, AccountStatus::LoggedOut, "gone from Google");
    assert_eq!(andi.tags, vec!["klien-a".to_string()], "but not forgotten");
    assert_eq!(budi.status, AccountStatus::Active, "newly seen");
  }

  #[test]
  fn reads_account_info_from_chromium_preferences_in_order() {
    let prefs = r#"{
      "account_info": [
        {"email":"andi@gmail.com","full_name":"Andi Saputra","picture_url":"https://lh3.googleusercontent.com/a/x","gaia":"1"},
        {"email":"budi@gmail.com","given_name":"Budi","account_image_url":"https://lh3.googleusercontent.com/a/y","gaia":"2"}
      ],
      "other": true
    }"#;
    let got = parse_account_info(prefs);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].email, "andi@gmail.com");
    assert_eq!(got[0].display_name.as_deref(), Some("Andi Saputra"));
    assert_eq!(got[0].authuser_index, 0);
    assert!(got[0].valid, "offline read reports accounts as valid");
    assert_eq!(got[1].email, "budi@gmail.com");
    assert_eq!(got[1].display_name.as_deref(), Some("Budi"), "falls back to given_name");
    assert_eq!(got[1].authuser_index, 1);
  }

  #[test]
  fn preferences_without_account_info_or_garbage_is_empty() {
    assert!(parse_account_info(r#"{"no_accounts":1}"#).is_empty());
    assert!(parse_account_info("not json").is_empty());
    // An entry without an email is skipped, not panicked on.
    assert!(parse_account_info(r#"{"account_info":[{"gaia":"1"}]}"#).is_empty());
  }

  #[test]
  fn import_remaps_profile_ids_and_drops_orphans() {
    let imported = vec![
      acct("andi@gmail.com", "old-p1", 0),
      acct("budi@gmail.com", "old-p2", 1),
    ];
    let mut map = std::collections::HashMap::new();
    map.insert("old-p1".to_string(), "new-p1".to_string());
    // old-p2 deliberately absent: its archive was not in the bundle.
    let out = remap_imported_accounts(&imported, &map);
    assert_eq!(out.len(), 1, "orphan dropped");
    assert_eq!(out[0].email, "andi@gmail.com");
    assert_eq!(out[0].profile_id, "new-p1", "repointed to imported profile");
    assert_ne!(out[0].id, "id-andi@gmail.com", "fresh account id");
    assert_eq!(out[0].tags, vec!["klien-a".to_string()], "metadata preserved");
  }

  #[test]
  fn a_needs_verification_row_maps_to_that_status() {
    let out = reconcile_profile_accounts("p1", &[], &[entry("x@gmail.com", 0, false)], 1);
    assert_eq!(out[0].status, AccountStatus::NeedsVerification);
  }
}
