import { invoke } from "@tauri-apps/api/core";

/** Mirrors `account_manager::AccountStatus` (serde serializes the variant name). */
export type AccountStatus =
  | "Active"
  | "NeedsVerification"
  | "LoggedOut"
  | "Unknown";

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
}

export const listGoogleAccounts = () =>
  invoke<GoogleAccount[]>("list_google_accounts");

export const openGoogleAccount = (accountId: string) =>
  invoke<void>("open_google_account", { accountId });

export const refreshProfile = (profileId: string) =>
  invoke<GoogleAccount[]>("refresh_profile", { profileId });

export const refreshAllAccounts = () =>
  invoke<GoogleAccount[]>("refresh_all_accounts");

/** Live Sync: launches each profile and reads ListAccounts from Google. */
export const syncAllAccountsLive = () =>
  invoke<GoogleAccount[]>("sync_all_accounts_live");

export const syncProfileLive = (profileId: string) =>
  invoke<GoogleAccount[]>("sync_profile_live", { profileId });

export const exportAccounts = (
  accountIds: string[],
  destination: string,
  password: string,
) => invoke<number>("export_accounts", { accountIds, destination, password });

export const importAccounts = (path: string, password: string) =>
  invoke<number>("import_accounts", { path, password });
