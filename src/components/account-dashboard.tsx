"use client";

import {
  open as openDialog,
  save as saveDialog,
} from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  LuDownload,
  LuExternalLink,
  LuFlag,
  LuRefreshCw,
  LuSearch,
  LuUpload,
} from "react-icons/lu";
import { LoadingButton } from "@/components/loading-button";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  type AccountStatus,
  type AntigravityStatus,
  exportAccounts,
  type GoogleAccount,
  importAccounts,
  keepAliveAll,
  listGoogleAccounts,
  openAntigravityAppeal,
  openGoogleAccount,
  setAntigravityStatus,
  syncAllAccountsLive,
} from "@/lib/accounts";
import { translateBackendError } from "@/lib/backend-errors";
import { showSuccessToast, showToast } from "@/lib/toast-utils";

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

const ANTIGRAVITY: Record<AntigravityStatus, string> = {
  Normal: "Antigravity OK",
  Banned: "Antigravity banned",
  AppealSent: "Appeal sent",
};

/** A session problem or an Antigravity ban: both need the user to act. */
function needsAttention(account: GoogleAccount): boolean {
  return STATUS[account.status].problem || account.antigravity === "Banned";
}

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

export function AccountDashboard() {
  const [accounts, setAccounts] = useState<GoogleAccount[]>([]);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [problemsOnly, setProblemsOnly] = useState(false);
  const [opening, setOpening] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [warming, setWarming] = useState(false);
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

  async function sync() {
    setSyncing(true);
    try {
      // Reads each profile's cookies and asks Google who is signed in.
      setAccounts(await syncAllAccountsLive());
    } catch (e) {
      showToast({ type: "error", title: translateBackendError(t, e) });
    } finally {
      setSyncing(false);
    }
  }

  async function keepAlive() {
    setWarming(true);
    try {
      const n = await keepAliveAll();
      // Re-sync so the list reflects the just-refreshed sessions: statuses are
      // re-checked and each row's timestamp flips to "just now" — the visible
      // proof that keep-alive ran.
      setAccounts(await syncAllAccountsLive());
      showSuccessToast(`Refreshed ${n} profile${n === 1 ? "" : "s"}`);
    } catch (e) {
      showToast({ type: "error", title: translateBackendError(t, e) });
    } finally {
      setWarming(false);
    }
  }

  async function changeAntigravity(
    account: GoogleAccount,
    status: AntigravityStatus,
  ) {
    try {
      setAccounts(await setAntigravityStatus(account.id, status));
    } catch (e) {
      showToast({ type: "error", title: translateBackendError(t, e) });
    }
  }

  // Google allows one appeal per account, so opening the form is confirmed.
  const [appealFor, setAppealFor] = useState<GoogleAccount | null>(null);

  async function confirmAppeal() {
    if (!appealFor) return;
    try {
      await openAntigravityAppeal(appealFor.id);
      setAppealFor(null);
    } catch (e) {
      showToast({ type: "error", title: translateBackendError(t, e) });
    }
  }

  // Password-gated export/import. `transfer` holds which flow is open and the
  // file path already chosen; the dialog only collects the password.
  const [transfer, setTransfer] = useState<{
    mode: "export" | "import";
    path: string;
  } | null>(null);
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);

  async function startExport() {
    const path = await saveDialog({
      filters: [{ name: "Roster", extensions: ["roster"] }],
      defaultPath: "accounts.roster",
    });
    if (path) {
      setPassword("");
      setTransfer({ mode: "export", path });
    }
  }

  async function startImport() {
    const path = await openDialog({
      filters: [{ name: "Roster", extensions: ["roster"] }],
      multiple: false,
    });
    if (typeof path === "string") {
      setPassword("");
      setTransfer({ mode: "import", path });
    }
  }

  async function confirmTransfer() {
    if (!transfer || !password) return;
    setBusy(true);
    try {
      if (transfer.mode === "export") {
        const n = await exportAccounts(
          accounts.map((a) => a.id),
          transfer.path,
          password,
        );
        showSuccessToast(`Exported ${n} account${n === 1 ? "" : "s"}`);
      } else {
        const n = await importAccounts(transfer.path, password);
        showSuccessToast(`Imported ${n} account${n === 1 ? "" : "s"}`);
        await sync();
      }
      setTransfer(null);
    } catch (e) {
      showToast({ type: "error", title: translateBackendError(t, e) });
    } finally {
      setBusy(false);
    }
  }

  // biome-ignore lint/correctness/useExhaustiveDependencies: load only on mount
  useEffect(() => {
    void load();
  }, []);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return accounts.filter((a) => {
      if (problemsOnly && !needsAttention(a)) return false;
      if (!q) return true;
      return (
        a.email.toLowerCase().includes(q) ||
        (a.display_name || "").toLowerCase().includes(q) ||
        (a.plan || "").toLowerCase().includes(q) ||
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

  const problemCount = accounts.filter(needsAttention).length;

  return (
    <div className="mx-auto flex h-full w-full max-w-4xl flex-col gap-4 p-4">
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
        <LoadingButton
          variant="outline"
          isLoading={syncing}
          onClick={() => void sync()}
        >
          <LuRefreshCw className="size-4" />
          Sync
        </LoadingButton>
        <LoadingButton
          variant="outline"
          isLoading={warming}
          onClick={() => void keepAlive()}
          title="Open each profile briefly so Google keeps the session alive"
        >
          Keep alive
        </LoadingButton>
        <Button variant="outline" onClick={() => void startExport()}>
          <LuUpload className="size-4" />
          Export
        </Button>
        <Button variant="outline" onClick={() => void startImport()}>
          <LuDownload className="size-4" />
          Import
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
                  <span className="flex min-w-0 items-center gap-1.5">
                    <span className="truncate font-medium text-sm">
                      {account.display_name || account.email}
                    </span>
                    <Badge
                      variant={
                        account.plan && account.plan !== "Free"
                          ? "default"
                          : "outline"
                      }
                      className="shrink-0 text-[10px]"
                      title={
                        account.plan
                          ? "Google AI plan, read from Google's account menu"
                          : "Plan not read yet — run Sync"
                      }
                    >
                      {account.plan ?? "?"}
                    </Badge>
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
                  <Select
                    value={account.antigravity}
                    onValueChange={(v) =>
                      void changeAntigravity(account, v as AntigravityStatus)
                    }
                  >
                    <SelectTrigger
                      size="sm"
                      aria-label="Antigravity status"
                      className={`h-8 w-40 text-xs ${
                        account.antigravity === "Banned"
                          ? "border-red-500 text-red-600"
                          : ""
                      }`}
                    >
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {(Object.keys(ANTIGRAVITY) as AntigravityStatus[]).map(
                        (s) => (
                          <SelectItem key={s} value={s} className="text-xs">
                            {ANTIGRAVITY[s]}
                          </SelectItem>
                        ),
                      )}
                    </SelectContent>
                  </Select>
                  {account.antigravity === "Banned" && (
                    <Button
                      size="sm"
                      variant="destructive"
                      onClick={() => setAppealFor(account)}
                    >
                      <LuFlag className="size-4" />
                      Appeal
                    </Button>
                  )}
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

      <Dialog
        open={appealFor !== null}
        onOpenChange={(o) => !o && setAppealFor(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Appeal Antigravity suspension</DialogTitle>
            <DialogDescription>
              Opens Google's appeal form in {appealFor?.email}'s own profile,
              signed in as that account. Google allows only one appeal per
              account, so fill it in completely before submitting. After you
              submit, set the status to "Appeal sent".
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setAppealFor(null)}>
              Cancel
            </Button>
            <Button onClick={() => void confirmAppeal()}>
              <LuExternalLink className="size-4" />
              Open appeal form
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={transfer !== null}
        onOpenChange={(o) => !o && setTransfer(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {transfer?.mode === "export"
                ? "Export accounts"
                : "Import accounts"}
            </DialogTitle>
            <DialogDescription>
              {transfer?.mode === "export"
                ? "Choose a password. You will need it to import this file on another computer."
                : "Enter the password this file was exported with."}
            </DialogDescription>
          </DialogHeader>
          <Input
            type="password"
            autoFocus
            placeholder="Password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void confirmTransfer();
            }}
          />
          <DialogFooter>
            <Button variant="ghost" onClick={() => setTransfer(null)}>
              Cancel
            </Button>
            <LoadingButton
              isLoading={busy}
              disabled={!password}
              onClick={() => void confirmTransfer()}
            >
              {transfer?.mode === "export" ? "Export" : "Import"}
            </LoadingButton>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
