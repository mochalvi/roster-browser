"use client";

import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { LuExternalLink, LuRefreshCw, LuSearch } from "react-icons/lu";
import { LoadingButton } from "@/components/loading-button";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  type AccountStatus,
  type GoogleAccount,
  listGoogleAccounts,
  openGoogleAccount,
} from "@/lib/accounts";
import { translateBackendError } from "@/lib/backend-errors";
import { showToast } from "@/lib/toast-utils";

const STATUS: Record<
  AccountStatus,
  { label: string; dot: string; problem: boolean }
> = {
  Active: { label: "Active", dot: "bg-emerald-500", problem: false },
  NeedsVerification: {
    label: "Needs verification",
    dot: "bg-amber-500",
    problem: true,
  },
  LoggedOut: { label: "Logged out", dot: "bg-red-500", problem: true },
  Unknown: {
    label: "Not checked",
    dot: "bg-muted-foreground/40",
    problem: false,
  },
};

function relativeTime(epochSeconds: number | null): string {
  if (!epochSeconds) return "never refreshed";
  const diff = Date.now() / 1000 - epochSeconds;
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

function initial(account: GoogleAccount): string {
  return (account.display_name || account.email || "?").charAt(0).toUpperCase();
}

export default function AccountsPage() {
  const [accounts, setAccounts] = useState<GoogleAccount[]>([]);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [problemsOnly, setProblemsOnly] = useState(false);
  const [opening, setOpening] = useState<string | null>(null);
  const { t } = useTranslation();

  async function load() {
    try {
      setAccounts(await listGoogleAccounts());
    } catch (e) {
      showToast({ type: "error", title: translateBackendError(t, e) });
    } finally {
      setLoading(false);
    }
  }

  // biome-ignore lint/correctness/useExhaustiveDependencies: load only on mount
  useEffect(() => {
    void load();
  }, []);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return accounts.filter((a) => {
      if (problemsOnly && !STATUS[a.status].problem) return false;
      if (!q) return true;
      return (
        a.email.toLowerCase().includes(q) ||
        (a.display_name || "").toLowerCase().includes(q) ||
        a.tags.some((t) => t.toLowerCase().includes(q))
      );
    });
  }, [accounts, query, problemsOnly]);

  async function onOpen(account: GoogleAccount) {
    setOpening(account.id);
    try {
      await openGoogleAccount(account.id);
    } catch (e) {
      showToast({ type: "error", title: translateBackendError(t, e) });
    } finally {
      setOpening(null);
    }
  }

  const problemCount = accounts.filter((a) => STATUS[a.status].problem).length;

  return (
    <div className="mx-auto flex h-screen w-full max-w-4xl flex-col gap-4 p-4">
      <header className="flex flex-col gap-1">
        <h1 className="text-xl font-semibold">Accounts</h1>
        <p className="text-sm text-muted-foreground">
          {accounts.length} Google account{accounts.length === 1 ? "" : "s"}
          {problemCount > 0 ? ` · ${problemCount} need attention` : ""}
        </p>
      </header>

      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <LuSearch className="-translate-y-1/2 absolute top-1/2 left-2 size-4 text-muted-foreground" />
          <Input
            className="pl-8"
            placeholder="Search email, name, or tag…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <Button
          variant={problemsOnly ? "default" : "outline"}
          onClick={() => setProblemsOnly((v) => !v)}
        >
          Problems only
        </Button>
        <Button variant="outline" onClick={() => void load()}>
          <LuRefreshCw className="size-4" />
          Reload
        </Button>
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto">
        {loading ? (
          <p className="py-8 text-center text-sm text-muted-foreground">
            Loading…
          </p>
        ) : filtered.length === 0 ? (
          <p className="py-8 text-center text-sm text-muted-foreground">
            {accounts.length === 0
              ? "No accounts yet. Add one to get started."
              : "No accounts match this filter."}
          </p>
        ) : (
          filtered.map((account) => {
            const status = STATUS[account.status];
            return (
              <div
                key={account.id}
                className="flex items-center gap-3 rounded-lg border bg-card p-3"
              >
                {account.avatar_url ? (
                  // biome-ignore lint/performance/noImgElement: Tauri app, no next/image
                  <img
                    src={account.avatar_url}
                    alt=""
                    className="size-9 rounded-full object-cover"
                  />
                ) : (
                  <div className="flex size-9 items-center justify-center rounded-full bg-muted font-medium text-sm">
                    {initial(account)}
                  </div>
                )}

                <div className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate font-medium text-sm">
                    {account.display_name || account.email}
                  </span>
                  <span className="truncate text-muted-foreground text-xs">
                    {account.email}
                  </span>
                  {account.tags.length > 0 && (
                    <div className="mt-1 flex flex-wrap gap-1">
                      {account.tags.map((tag) => (
                        <Badge
                          key={tag}
                          variant="secondary"
                          className="text-[10px]"
                        >
                          {tag}
                        </Badge>
                      ))}
                    </div>
                  )}
                </div>

                <div className="flex items-center gap-2">
                  <span
                    className="flex items-center gap-1.5 text-muted-foreground text-xs"
                    title={status.label}
                  >
                    <span className={`size-2 rounded-full ${status.dot}`} />
                    {status.label}
                  </span>
                  <span className="hidden text-muted-foreground text-xs sm:inline">
                    {relativeTime(account.last_refresh)}
                  </span>
                  <LoadingButton
                    size="sm"
                    isLoading={opening === account.id}
                    onClick={() => void onOpen(account)}
                  >
                    <LuExternalLink className="size-4" />
                    Open
                  </LoadingButton>
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
