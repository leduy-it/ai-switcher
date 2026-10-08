import { useState } from "react";
import { CheckCircle2, CircleHelp, ExternalLink, Loader2, Monitor, RefreshCw, X } from "lucide-react";
import { api } from "./tauri";
import type { AppSnapshot, DesktopApp } from "./types";
import "./desktop-status.css";

const names: Record<DesktopApp, string> = { codex: "Codex.app", chatgpt: "ChatGPT.app" };

export function confirmedDesktopEmail(snapshot: AppSnapshot): string | null {
  const sync = snapshot.desktopSync;
  const tool = snapshot.tools.find((t) => t.id === "codex");
  const selected = tool?.accounts.find((a) => a.id === tool.activeAccountId);
  const matching = snapshot.desktops?.some((d) => d.app === sync?.settings.app && d.running && d.profileHome === snapshot.selectedCodexHome && d.sessionHome === snapshot.sharedCodexHome);
  return matching && sync?.confirmedAt && !sync.error && sync.lastAppliedAccountId === selected?.id && sync.confirmedEmail && sync.confirmedEmail.toLowerCase() === selected?.accountEmail?.toLowerCase() ? sync.confirmedEmail : null;
}

export function DesktopStatus({ snapshot, onUpdate, compact = false }: { snapshot: AppSnapshot; onUpdate: (value: AppSnapshot) => void; compact?: boolean }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [repairResult, setRepairResult] = useState<string | null>(null);
  const sync = snapshot.desktopSync;
  if (!sync) return null;
  const tool = snapshot.tools.find((t) => t.id === "codex");
  const selected = tool?.accounts.find((a) => a.id === tool.activeAccountId);
  const operation = sync.operation;
  const matching = snapshot.desktops?.find((d) => d.running && d.profileHome === snapshot.selectedCodexHome && d.sessionHome === snapshot.sharedCodexHome);
  const confirmed = confirmedDesktopEmail(snapshot);
  const perform = async (task: () => Promise<AppSnapshot>) => {
    if (busy) return; setBusy(true); setError(null);
    try { onUpdate(await task()); } catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  const inProgress = operation && ["checkpointing", "switching", "restoring", "retrySwitch"].includes(operation.phase);
  const pending = inProgress || operation?.phase === "waiting";
  const repair = async () => {
    if (busy) return; setBusy(true); setError(null); setRepairResult(null);
    try {
      const result = await api.repairCodexSessions();
      setRepairResult(`${result.profiles} profile catalogs checked · ${result.catalogThreads} sessions and ${result.historyThreads} histories added. ${result.conflicts ? `${result.conflicts} partial histories need attention. ` : ""}${result.pendingProjection ? `${result.pendingProjection} histories have pending metadata; open them in Desktop to refresh. ` : ""}${result.unavailableRollouts ? `${result.unavailableRollouts} unavailable rollout files left untouched. ` : ""}${result.backups.length ? "Private backups saved in the main Codex backups folder." : "Existing records preserved."}`);
    } catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  return <section className={`desktopStatus ${compact ? "desktopStatusCompact" : ""}`}>
    <div className="desktopStatusHead"><Monitor size={17} /><strong>Codex desktop account</strong>{busy && <Loader2 size={14} className="spin" />}</div>
    <div className="desktopIdentity"><span>CLI selected</span><strong>{selected?.name ?? "Not selected"}</strong><small>{selected?.accountEmail ?? "Email unavailable"}</small></div>
    <div className={`desktopIdentity ${confirmed ? "desktopConfirmed" : ""}`}><span>{confirmed ? <CheckCircle2 size={13} /> : <CircleHelp size={13} />}Desktop backend</span><strong>{confirmed ? sync.confirmedEmail : matching ? "Profile launched · identity unconfirmed" : "Account has not been confirmed"}</strong><small>{confirmed ? `${sync.confirmedPlan ?? "Plan unavailable"} · confirmed ${formatTime(sync.confirmedAt)}` : "Selecting the CLI alone does not confirm the desktop account."}</small></div>
    {!compact && <div className="desktopRuntimeRows">{snapshot.desktops?.map((desktop) => <div key={`${desktop.app}:${desktop.pid}`}><strong>{names[desktop.app]}</strong><span>{!desktop.installed ? "Not installed" : !desktop.running ? "Closed" : desktop.profileHome ? desktop.profileHome === snapshot.selectedCodexHome ? "Selected profile is running" : "Different profile is running" : "Running · unmanaged launch"}</span></div>)}</div>}
    <div className="desktopControls"><label><input type="checkbox" checked={sync.settings.enabled} disabled={busy || !!pending} onChange={(e) => void perform(() => api.setDesktopSync({ ...sync.settings, enabled: e.target.checked }))} />Apply desktop when selecting an account</label><select aria-label="Desktop app to update" value={sync.settings.app} disabled={busy || !!pending} onChange={(e) => void perform(() => api.setDesktopSync({ ...sync.settings, app: e.target.value as DesktopApp }))}>{snapshot.desktops?.filter((d, index, all) => d.installed && all.findIndex((item) => item.app === d.app) === index).map((d) => <option key={`${d.app}:${d.pid}`} value={d.app}>{names[d.app]}</option>)}</select><button disabled={busy || !!pending || !selected || !!selected.apiProvider} onClick={() => void perform(() => api.applyCodexDesktop(sync.settings.app))}><RefreshCw size={13} />Apply desktop</button></div>
    {!compact && <p className="desktopHint">Waits for active turns by default. The chosen desktop restarts with the selected profile and the main session catalog. Both installed brands use the same desktop data folder, so one runs at a time. A busy profile backend is left running.</p>}
    {!compact && <div className="desktopCatalogRepair"><button disabled={busy || !!pending} onClick={() => void repair()}><RefreshCw size={13} />Recover missing local sessions</button><small>Checks profile catalogs and paginated history. Adds missing records with private backups; existing sessions keep their identities.</small>{repairResult && <p role="status">{repairResult}</p>}</div>}
    {operation && <details className="desktopOperation" open={operation.phase !== "applied" && operation.phase !== "cancelled"}><summary>{phaseName(operation.phase)} · {operation.sessions.length} session(s)</summary><p>{operation.message}</p>{operation.sessions.map((session) => <div className="desktopRecoverySession" key={`${session.threadId}:${session.cwd}`}><strong>{session.name}</strong><span>{phaseName(session.phase)} · {session.message}</span><small>{session.cwd}</small><button onClick={() => void api.openDesktopThread(session.threadId).catch((e) => setError(String(e)))}><ExternalLink size={12} />Open original session</button></div>)}<div className="desktopOperationActions">{operation.phase === "waiting" && <><button disabled={busy} onClick={() => void perform(() => api.desktopSwitchAction("cancel"))}><X size={12} />Cancel pending switch</button><button disabled={busy} title="Save a private checkpoint, interrupt only recoverable local desktop turns, then request continuation in their original threads. Sessions needing approval or unsupported runtimes stay untouched." onClick={() => void perform(() => api.desktopSwitchAction("switchNow"))}>Switch now &amp; recover</button></>}{operation.phase === "needsAttention" && <><button disabled={busy} onClick={() => void perform(() => api.desktopSwitchAction("retry"))}>Retry recovery safely</button><button disabled={busy} onClick={() => void perform(() => api.desktopSwitchAction("dismiss"))}>Dismiss recovery</button></>}</div><small>Operation {operation.id} · {formatTime(operation.updatedAt)}</small></details>}
    {(error || sync.error) && <p className="desktopError">{error || sync.error}</p>}
    <small className="desktopQuotaTime">Selected account quota updated: {formatTime(selected?.quota?.updatedAt)}{selected?.quota?.error ? ` · ${selected.quota.error}` : ""}</small>
  </section>;
}

function formatTime(value?: string | null) { if (!value) return "Not yet"; const date = new Date(value); return Number.isNaN(date.getTime()) ? "Unavailable" : date.toLocaleTimeString(); }
export function phaseName(phase: string) { return ({ waiting: "Waiting to switch", checkpointing: "Saving state", switching: "Changing account", restoring: "Restoring", retrySwitch: "Retrying handoff", applied: "Applied", continued: "Continued", needsAttention: "Needs attention", interrupted: "Interrupted by this switch", completed: "Completed", cancelled: "Cancelled", notResumed: "Left for original session" } as Record<string, string>)[phase] ?? phase; }
