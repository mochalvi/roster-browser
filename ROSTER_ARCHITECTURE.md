# Roster Architecture (fork delta over Donut Browser)

Roster adds **one concept** to Donut: a *Google account* that points at a
(profile, authuser-index) pair. Everything else — profiles, proxies,
fingerprints, launching, cookie keep-alive, portable export — is Donut's and is
reused, not rewritten. This file tracks what is done and what remains, with the
exact Donut reuse points so the remaining work stays small.

## Status

| Area | State | Where |
|---|---|---|
| Account domain model + slot allocation + ListAccounts parser + reconcile | **Done, unit-tested** | `src-tauri/src/account_manager.rs` |
| Rebrand: name, window title, logo component, locale brand strings, README EN/ID | **Done** | `tauri.conf.json`, `lib.rs`, `src/components/icons/logo.tsx`, `src/i18n/locales/*`, `README.md` |
| App icons (.ico/.icns/png set) | **Pending** — generate with `pnpm dlx @tauri-apps/cli icon branding/logo.png` at build | `src-tauri/icons/` |
| Tauri commands (account surface) | **Pending** | new `src-tauri/src/account_commands.rs` |
| Refresh / sync engine | **Pending** — reuse `cookie_bot` | — |
| Export / import `.roster` | **Pending** — reuse `profile/portable.rs` + `sync::encryption` | — |
| Import from Chrome/Edge/Firefox | **Pending** — reuse `profile_importer.rs` + `profile_import/os_crypt.rs` | — |
| Frontend account dashboard | **Pending** | `src/app/page.tsx` (new account view) |
| Indonesian UI locale (`id.json`) | **Deferred** — i18n-parity test needs full key coverage | `src/i18n/locales/id.json` |

## Commands to add (`account_commands.rs`)

Every command must be invoked by the frontend or `check-unused-commands` fails.

1. `list_google_accounts() -> Vec<GoogleAccount>`
   → `AccountManager::instance().list()`.

2. `open_google_account(account_id)`
   → look up account → its profile → `browser_runner::launch_browser_profile(
   app, profile, Some(open_account_url(idx)), None)`. If the profile is already
   running, Donut's `launch_or_open_url` opens a new tab in the same window.

3. `add_google_account(group_id: Option<String>, capacity: usize)`
   → `usage_by_profile()` → `pick_profile_with_free_slot()`; if `None`, create a
   hidden slot profile via `ProfileManager::create_profile_with_group(app,
   name, "wayfern", version, "stable", proxy_id, None, wayfern_cfg, group_id,
   false, None, None, None)` — `"wayfern"` is the only supported engine
   (Camoufox was removed). Flag it hidden via a Roster tag or `group_id`. Launch
   at `add_session_url()` for a
   profile that already has accounts, else plain `google.com` sign-in. User logs
   in manually; the next refresh detects the new email/avatar.

4. `refresh_profile(profile_id)` and `refresh_all(limit)`
   → see Refresh engine.

5. `export_accounts(path, password, account_ids)` /
   `preview_roster(path)` / `import_accounts(path, password, wipe: true)`
   → see Export/import.

6. `set_account_meta(account_id, tags?, note?)`
   → `AccountManager::set_account_meta`.

7. `import_accounts_from_browser(source, ...)`
   → see Browser import.

## Refresh / sync engine (local headless launch)

Refresh is per *profile* (one launch warms up to 10 accounts at once).

> Note: `cookie_bot::run_now` POSTs to Donut's **daemon/remote** (`{base}/runs`),
> so it is not a local launch. Roster refreshes locally instead.

1. Launch the profile headless locally via `browser_runner` with
   `LaunchOptions::automation(port, headless=true)`, driving it through
   `wayfern_cdp` (the CDP channel Donut already uses). Visiting a Google page
   rotates the session cookies, the same keep-alive effect.
2. Read account state back. Two options, prefer (a):
   - **(a) Network ListAccounts**: while the profile is up, fetch
     `https://accounts.google.com/ListAccounts?json=standard` through the
     profile's context (CDP `Network`/`fetch` on the launched browser, or the
     existing automation channel cookie_bot uses). Feed body to
     `account_manager::parse_list_accounts`.
   - **(b) Offline cookie read**: `cookie_health::inspect_cookies(cookies_path,
     key)` to confirm Google auth cookies exist. Coarser (no per-account
     validity), use only as a fallback when the profile cannot be launched.
3. `AccountManager::apply_profile_refresh(profile_id, entries, now_secs())`.
4. `events::emit("roster://account-refreshed", payload)` per account so the UI
   flips ⏳→🔄→✅/⚠️/❌ live. `refresh_all` runs at most `limit` profiles at once.

## Export / import `.roster` (reuse portable + encryption)

- **Export** `account_ids` → resolve their distinct profiles →
  `profile::portable::export_blocking` each into a temp dir → add
  `accounts.json` (the `GoogleAccount`s for those profiles) and a manifest →
  zip → encrypt the zip with `sync::encryption` (password-derived key) → write
  `.roster`. **Move, not copy**: after a verified write, mark the source
  profiles locked/disabled so the same session is not run in two places (the #1
  cause of Google logout). Refresh accounts first so the moved session is fresh.
- **Import** (`wipe: true`): all browsers must be closed →
  decrypt + verify integrity *before* touching anything (wrong password/corrupt
  = abort, nothing deleted) → back up current profiles+accounts (encrypted) →
  user confirms by typing `HAPUS`/`DELETE` → atomically swap in the imported
  profiles via `profile::portable::import_profile_archive` and replace
  `accounts.json` → refresh all → show ✅/⚠️/❌. Backup auto-deletes after 7 days
  or on user confirmation.

## Browser import (reuse `profile_importer` + `os_crypt`)

Donut's `profile_importer.rs` already imports Chrome/Edge/Firefox profiles and
`profile_import/os_crypt.rs` unseals Chromium cookies. Onboarding step:

1. Detect installed Chrome/Edge/Firefox profiles.
2. Import each as a Roster slot profile (Donut importer).
3. Refresh to enumerate Google accounts into the account list.
4. Show the honesty labels from the README (Firefox reliable; Chrome/Edge
   best-effort due to App-Bound Encryption). Warn: stop using the account in its
   original browser (two-places logout risk).

## Frontend (`src/app/page.tsx` account view)

- Account cards/list: avatar, email, status badge, tags, last-refresh. Virtualized
  for hundreds of rows (Donut already uses a virtualized profile table — reuse
  that component).
- Search + filter (reuse Donut `profile-search`). "Show problems only" filter for
  ⚠️/❌.
- Buttons: Open (→ `open_google_account`), per-account Refresh, Refresh All (bounded),
  Add account, Export, Import, Settings → Advanced → Profiles (the hidden slot view).
- Sync progress bar: `Sync 37/120 · ✅ 35 · ⚠️ 1 · ❌ 1`, driven by the
  `roster://account-refreshed` events.

## Constraints learned from the codebase

- `check-unused-commands` test: no `generate_handler!` command may be unused by
  the frontend.
- i18n-parity test: every `src/i18n/locales/*.json` must have identical keys —
  so Indonesian UI is all-or-nothing.
- Internal names (`donut-sync`, `DonutLab`, crate `donutbrowser_lib`) are kept;
  renaming them is churn with no user benefit and risks the test suite.
- `lint:rust` runs clippy with `-D warnings`; unused pub items warn, so land
  account code together with the commands that use it.
