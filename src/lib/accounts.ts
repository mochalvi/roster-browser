import { invoke } from "@tauri-apps/api/core";

/** Mirrors `account_manager::AccountStatus` (serde serializes the variant name). */
export type AccountStatus =
  | "Active"
  | "NeedsVerification"
  | "LoggedOut"
  | "Unknown";

/** Mirrors `account_manager::AntigravityStatus` (set by the user). */
export type AntigravityStatus = "Normal" | "Banned" | "AppealSent";

/** Mirrors `account_manager::GoogleAccount`. */
export interface GoogleAccount {
  id: string;
  email: string;
  display_name: string | null;
  avatar_url: string | null;
  profile_id: string;
  authuser_index: number;
  status: AccountStatus;
  last_refresh: number | null;
  tags: string[];
  note: string | null;
  /** "Pro", "Ultra", ... from Google's account menu, "Free", or null if unread. */
  plan: string | null;
  antigravity: AntigravityStatus;
  antigravity_since: number | null;
}

export const listGoogleAccounts = () =>
  invoke<GoogleAccount[]>("list_google_accounts");

export const openGoogleAccount = (accountId: string) =>
  invoke<void>("open_google_account", { accountId });

export const setAntigravityStatus = (
  accountId: string,
  status: AntigravityStatus,
) => invoke<GoogleAccount[]>("set_antigravity_status", { accountId, status });

/** Opens Google's appeal form in the account's own profile, as that account. */
export const openAntigravityAppeal = (accountId: string) =>
  invoke<void>("open_antigravity_appeal", { accountId });

export const refreshProfile = (profileId: string) =>
  invoke<GoogleAccount[]>("refresh_profile", { profileId });

export const refreshAllAccounts = () =>
  invoke<GoogleAccount[]>("refresh_all_accounts");

/** Live Sync: launches each profile and reads ListAccounts from Google. */
export const syncAllAccountsLive = () =>
  invoke<GoogleAccount[]>("sync_all_accounts_live");

export const syncProfileLive = (profileId: string) =>
  invoke<GoogleAccount[]>("sync_profile_live", { profileId });

/** Keep sessions warm: briefly open each profile at a Google page. */
export const keepAliveAll = () => invoke<number>("keep_alive_all");

export const exportAccounts = (
  accountIds: string[],
  destination: string,
  password: string,
) => invoke<number>("export_accounts", { accountIds, destination, password });

export const importAccounts = (path: string, password: string) =>
  invoke<number>("import_accounts", { path, password });
