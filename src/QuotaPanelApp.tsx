import { Fragment, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ArrowUpRight, ChevronDown, ChevronRight, ChevronsDownUp, ChevronsUpDown, Loader2, Maximize2, Pin, RefreshCw, Search, X } from "lucide-react";
import michaelLogo from "./assets/logo-michael.svg";
import { api } from "./tauri";
import { DesktopStatus } from "./DesktopStatus";
import type { Account, AppSnapshot, QuotaWindow, ToolId, ToolStatus } from "./types";
import "./quota-panel.css";

const toolNames: Record<ToolId, string> = { codex: "Codex", claude: "Claude", cursor: "Cursor", opencode: "opencode", antigravity: "Antigravity" };

export function QuotaPanelApp() {
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState<ToolId | "all">("all");
  const [search, setSearch] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [anchor, setAnchor] = useState(360);
  const [, setTick] = useState(0);

  useEffect(() => {
    let cancelled = false;
    const reload = () => void api.getSnapshot().then((next) => { if (!cancelled) setSnapshot(next); }).catch(() => { if (!cancelled) setMessage("Could not load accounts. Try Refresh."); });
    const listeners = [
      listen<AppSnapshot>("snapshot-changed", (event) => setSnapshot(event.payload)),
      listen("quota-panel-opened", reload),
      listen<number>("quota-panel-anchor", (event) => setAnchor(event.payload)),
    ];
    reload();
    const timer = window.setInterval(() => setTick((value) => value + 1), 30_000);
    const onKey = (event: KeyboardEvent) => { if (event.key === "Escape") void api.closeQuotaPanel(); };
    window.addEventListener("keydown", onKey);
    return () => { cancelled = true; window.clearInterval(timer); window.removeEventListener("keydown", onKey); listeners.forEach((listener) => void listener.then((stop) => stop())); };
  }, []);

  const rows = useMemo(() => snapshot?.tools.flatMap((tool) => tool.accounts.filter((account) => !account.hidden).map((account) => ({ tool, account, key: `${tool.id}:${account.id}` }))) ?? [], [snapshot]);
  const visible = rows.filter(({ tool, account }) => (filter === "all" || filter === tool.id) && `${account.name} ${account.accountEmail ?? ""} ${toolNames[tool.id]}`.toLowerCase().includes(search.toLowerCase()));
  const tools = snapshot?.tools.filter((tool) => tool.accounts.some((account) => !account.hidden)) ?? [];
  const allExpanded = visible.length > 0 && visible.every(({ key }) => expanded.has(key));

  const perform = async (key: string, action: () => Promise<unknown>) => {
    if (busy) return;
    setBusy(key); setMessage(null);
    try { await action(); setSnapshot(await api.getSnapshot()); }
    catch (error) { setMessage(String(error)); }
    finally { setBusy(null); }
  };

  const refresh = () => perform("refresh", async () => {
    const results = await Promise.allSettled(tools.filter((tool) => tool.installed).map((tool) => api.refreshTool(tool.id)));
    if (results.some((result) => result.status === "rejected")) setMessage("Some quota reports could not refresh. Expand the account for details.");
  });
  const toggle = (key: string) => setExpanded((previous) => { const next = new Set(previous); if (next.has(key)) next.delete(key); else next.add(key); return next; });
  const openApp = (fullscreen: boolean) => void api.openMainWindow(fullscreen).catch((error) => setMessage(String(error)));
  const pin = () => perform("pin", async () => { await api.setOverlayEnabled(true); await api.closeQuotaPanel(); });

  return (
    <section className="qpRoot" style={{ "--qp-anchor": `${anchor}px` } as React.CSSProperties} aria-label="Account quota dropdown">
      <header className="qpHeader">
        <img src={michaelLogo} alt="" />
        <div><strong>Michael Le Profiles</strong><span>Accounts & quota</span></div>
        <button className="qpIcon" title="Pin floating quota overlay" aria-label="Pin floating quota overlay" onClick={pin} disabled={busy !== null}><Pin size={16} /></button>
        <button className="qpIcon" title="Refresh quotas" aria-label="Refresh quotas" onClick={() => void refresh()} disabled={busy !== null}>{busy === "refresh" ? <Loader2 className="qpSpin" size={16} /> : <RefreshCw size={16} />}</button>
        <button className="qpIcon" title="Close dropdown" aria-label="Close dropdown" onClick={() => void api.closeQuotaPanel()}><X size={16} /></button>
      </header>
      <div className="qpFilters">
        <nav aria-label="Filter provider"><button className={filter === "all" ? "selected" : ""} onClick={() => setFilter("all")}>All <span>{rows.length}</span></button>{tools.map((tool) => <button key={tool.id} className={filter === tool.id ? "selected" : ""} onClick={() => setFilter(tool.id)}>{toolNames[tool.id]}</button>)}</nav>
        <label className="qpSearch"><Search size={14} /><input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Find name or email" aria-label="Find account by name or email" /></label>
      </div>
      <div className="qpCaption"><span>{visible.length} accounts · quota used</span><button onClick={() => setExpanded(allExpanded ? new Set() : new Set(visible.map(({ key }) => key)))}>{allExpanded ? <ChevronsDownUp size={13} /> : <ChevronsUpDown size={13} />}{allExpanded ? "Collapse all" : "Expand all"}</button></div>
      {message && <p className="qpMessage" role="status">{message}</p>}
      <div className="qpScroll">
        {snapshot && (filter === "all" || filter === "codex") && <DesktopStatus snapshot={snapshot} onUpdate={setSnapshot} compact />}
        <table className="qpTable">
          <thead><tr><th scope="col">Account / email</th><th scope="col">Plan</th><th scope="col">5-hour</th><th scope="col">Weekly</th></tr></thead>
          <tbody>
            {visible.map(({ tool, account, key }) => {
              const active = tool.activeAccountId === account.id;
              const quota = account.quota;
              const stale = Boolean(quota?.error);
              return <Fragment key={key}>
                <tr className={`${active ? "qpActive" : ""} ${expanded.has(key) ? "qpExpanded" : ""}`}>
                  <td><button className="qpAccount" onClick={() => toggle(key)} aria-expanded={expanded.has(key)} aria-controls={`detail-${key}`}>
                    {expanded.has(key) ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
                    <span className="qpIdentity"><span className="qpName">{account.name}<small>{toolNames[tool.id]}</small>{active && <i title="In use" />}</span><span className="qpEmail" title={account.accountEmail ?? undefined}>{account.accountEmail || "Email unavailable"}</span></span>
                  </button></td>
                  <td><span className="qpPlan">{quota?.plan || (account.apiProvider ? "API" : "—")}</span>{(stale || account.state === "needs-login" || account.weeklyLock?.locked) && <span className="qpIssue">{account.weeklyLock?.locked ? "Locked" : /40[13]/.test(quota?.error ?? "") || account.state === "needs-login" ? "Sign in" : "Read error"}</span>}</td>
                  <td><QuotaCell window={quota?.fiveHour} stale={stale} api={Boolean(account.apiProvider)} /></td>
                  <td><QuotaCell window={quota?.weekly} stale={stale} api={Boolean(account.apiProvider)} /></td>
                </tr>
                {expanded.has(key) && <tr className="qpDetailRow" id={`detail-${key}`}><td colSpan={4}><AccountDetails tool={tool} account={account} busy={busy !== null} onRefresh={() => void perform(key, () => api.refreshAccount(tool.id, account.id))} onSwitch={() => void perform(key, () => api.switchAccount({ toolId: tool.id, accountId: account.id }))} /></td></tr>}
              </Fragment>;
            })}
          </tbody>
        </table>
        {snapshot === null ? <p className="qpEmpty"><Loader2 size={18} className="qpSpin" />Loading accounts…</p> : visible.length === 0 && <p className="qpEmpty">{rows.length ? "No accounts match this filter." : "Add an account in the full app to see its usage here."}</p>}
      </div>
      <footer className="qpFooter"><span>Click an account to expand its details</span><button onClick={() => openApp(false)}>Open app <ArrowUpRight size={15} /></button><button className="qpPrimary" onClick={() => openApp(true)}>Full screen <Maximize2 size={14} /></button></footer>
    </section>
  );
}

function QuotaCell({ window, stale, api: isApi }: { window?: QuotaWindow; stale: boolean; api: boolean }) {
  const percent = window?.percentUsed;
  const value = Math.max(0, Math.min(100, percent ?? 0));
  return <div className={`qpQuota ${stale ? "stale" : ""}`} title={window?.resetAt ? `Resets ${formatTime(window.resetAt)}` : undefined}><strong>{isApi ? "API" : percent == null ? "—" : `${Math.round(value)}%`}</strong><span className="qpTrack" data-level={value >= 90 ? "high" : value >= 70 ? "mid" : "low"}><span style={{ width: `${value}%` }} /></span><small>{window?.resetAt ? countdown(window.resetAt) : "No reset reported"}</small></div>;
}

function AccountDetails({ tool, account, busy, onRefresh, onSwitch }: { tool: ToolStatus; account: Account; busy: boolean; onRefresh: () => void; onSwitch: () => void }) {
  const active = tool.activeAccountId === account.id;
  const quota = account.quota;
  const canSwitch = !active && tool.id !== "antigravity" && account.state !== "needs-login" && !account.weeklyLock?.locked;
  const windows = quota?.models?.length ? quota.models : quota ? [quota.fiveHour, quota.weekly] : [];
  return <div className="qpDetails"><dl><div><dt>Status</dt><dd>{account.weeklyLock?.locked ? "Weekly quota locked" : active ? "In use" : account.state === "needs-login" ? "Needs sign in" : account.state === "exhausted" ? "Quota exhausted" : "Ready"}</dd></div><div><dt>Command</dt><dd><code>{account.launcherCommand || (account.isDefault ? tool.id : "No launcher configured")}</code></dd></div><div><dt>Last quota read</dt><dd>{formatTime(quota?.updatedAt)}</dd></div><div><dt>Last used</dt><dd>{formatTime(account.lastUsedAt)}</dd></div>{windows.map((window, index) => <div key={`${window.label}-${index}`}><dt>{window.label || (index === 0 ? "5-hour" : "Weekly")} reset</dt><dd>{formatTime(window.resetAt)}{quota?.models?.length && window.percentUsed != null ? ` · ${Math.round(window.percentUsed)}% used` : ""}</dd></div>)}{quota?.rateLimitResetCredits && <div><dt>Reset credits</dt><dd>{quota.rateLimitResetCredits.availableCount} available</dd></div>}</dl>{quota?.error && <p className="qpError">{quota.error}{quota.rateLimitedUntil && ` · Retry after ${formatTime(quota.rateLimitedUntil)}`}</p>}{account.apiProvider && <p className="qpNote">API gateway · {account.apiProvider.model}</p>}<div className="qpActions"><button onClick={onRefresh} disabled={busy}><RefreshCw size={13} />Refresh account</button>{canSwitch && <button className="qpPrimary" onClick={onSwitch} disabled={busy}>Use this account</button>}{tool.id === "antigravity" && <span>Switch this account in the full app.</span>}</div></div>;
}

function formatTime(value?: string | null) {
  if (!value) return "Not available";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "Not available" : new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(date);
}
function countdown(value: string) {
  const remaining = Math.ceil((new Date(value).getTime() - Date.now()) / 60_000);
  if (!Number.isFinite(remaining)) return "No reset reported";
  if (remaining <= 0) return "Reset due";
  if (remaining < 60) return `Resets in ${remaining}m`;
  const hours = Math.floor(remaining / 60);
  return hours >= 24 ? `Resets in ${Math.floor(hours / 24)}d ${hours % 24}h` : `Resets in ${hours}h ${remaining % 60}m`;
}
