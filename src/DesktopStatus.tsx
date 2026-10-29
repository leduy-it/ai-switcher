import { useState } from "react";
import { CheckCircle2, CircleHelp, ExternalLink, Loader2, Monitor, RefreshCw, Settings, X } from "lucide-react";
import { api } from "./tauri";
import { desktopStatusState } from "./desktop-status-state";
import { LocalChatFinder } from "./LocalChatFinder";
import type { AppSnapshot, DesktopApp } from "./types";
import "./desktop-status.css";

const names: Record<DesktopApp, string> = { codex: "Codex.app", chatgpt: "ChatGPT.app" };

export function confirmedDesktopEmail(snapshot: AppSnapshot): string | null {
  return desktopStatusState(snapshot).confirmedEmail;
}

export function DesktopStatus({ snapshot, onUpdate, compact = false }: { snapshot: AppSnapshot; onUpdate: (value: AppSnapshot) => void; compact?: boolean }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [repairResult, setRepairResult] = useState<string | null>(null);
  const sync = snapshot.desktopSync;
  if (!sync) return null;
  const { selected, matching, confirmedEmail: confirmed, actualEmail, settingsLocked, privateBackend, canSwitchNow, pending } = desktopStatusState(snapshot);
  const operation = sync.operation;
  const perform = async (task: () => Promise<AppSnapshot>) => {
    if (busy) return; setBusy(true); setError(null);
    try { onUpdate(await task()); } catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  const repair = async () => {
    if (busy) return; setBusy(true); setError(null); setRepairResult(null);
    try {
      const result = await api.repairCodexSessions();
      setRepairResult(`${result.profiles} profile catalogs checked · ${result.catalogThreads} sessions and ${result.historyThreads} histories added. ${result.desktopStateProfiles ? `Sidebar records recovered from ${result.desktopStateProfiles} profiles. Finish active work, then reapply the desktop account to refresh its chat list. ` : ""}${result.conflicts ? `${result.conflicts} partial histories need attention. ` : ""}${result.pendingProjection ? `${result.pendingProjection} histories have pending metadata; open them in Desktop to refresh. ` : ""}${result.unavailableRollouts ? `${result.unavailableRollouts} unavailable rollout files left untouched. ` : ""}${result.backups.length || result.desktopStateProfiles ? "Private backups saved in the main Codex backups folder." : "Existing records preserved."}`);
    } catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  return <section className={`desktopStatus ${compact ? "desktopStatusCompact" : ""}`}>
    <div className="desktopStatusHead"><Monitor size={17} /><strong>Codex desktop</strong><span className={`desktopStatusBadge ${confirmed ? "verified" : privateBackend ? "warning" : ""}`}>{confirmed ? "Account verified" : privateBackend ? "Setup required" : matching ? "Checking account" : "Not applied"}</span>{busy && <Loader2 size={14} className="spin" />}</div>
    <div className="desktopSummary"><span>Selected: <strong>{selected?.name ?? "No account"}</strong></span><span>{actualEmail ? <>Desktop: <strong>{actualEmail}</strong></> : "Desktop account is not confirmed"}</span></div>
    {sync.autoSwitchMessage && <p className="desktopAutoNotice" role="status">{sync.autoSwitchMessage}</p>}
    {operation && !["applied", "cancelled"].includes(operation.phase) && <div className={`desktopPending ${operation.phase === "needsAttention" ? "desktopNeedsAttention" : ""}`} role="status">
      <strong>{phaseName(operation.phase)}</strong><p>{operation.message}</p>
      <div className="desktopOperationActions">{operation.phase === "waiting" && <><button disabled={busy || settingsLocked} onClick={() => void perform(() => api.desktopSwitchAction("cancel"))}><X size={12} />Cancel pending switch</button><button disabled={busy || !canSwitchNow} title={!canSwitchNow ? operation.message : "Checkpoint eligible chats and continue them after a verified account switch"} onClick={() => void perform(() => api.desktopSwitchAction("switchNow"))}>Switch now &amp; continue</button></>}{operation.phase === "needsAttention" && <><button disabled={busy} onClick={() => void perform(() => api.desktopSwitchAction("retry"))}>Retry safely</button><button disabled={busy} onClick={() => void perform(() => api.desktopSwitchAction("dismiss"))} title="Dismiss this status; any continuation already delivered to the desktop stays under its control.">Dismiss status</button></>}</div>
    </div>}
    {(error || sync.error) && <p className="desktopError" role="alert">{error || sync.error}</p>}
    <details className="desktopSettings"><summary><Settings size={13} />Desktop settings &amp; details</summary>
      <div className="desktopIdentity"><span>Selected account</span><strong>{selected?.name ?? "Not selected"}</strong><small>{selected?.accountEmail ?? "Email unavailable"}</small></div>
      <div className={`desktopIdentity ${confirmed ? "desktopConfirmed" : ""}`}><span>{confirmed ? <CheckCircle2 size={13} /> : <CircleHelp size={13} />}Desktop backend</span><strong>{confirmed ? sync.confirmedEmail : actualEmail ? `Still using ${actualEmail}` : matching ? "Profile launched; identity unconfirmed" : "Account has not been confirmed"}</strong><small>{confirmed ? `${sync.confirmedPlan ?? "Plan unavailable"} · confirmed ${formatTime(sync.confirmedAt)}` : "Selecting an account alone does not confirm that the desktop has changed."}</small></div>
      {!compact && <div className="desktopRuntimeRows">{snapshot.desktops?.map((desktop) => <div key={`${desktop.app}:${desktop.pid}`}><strong>{names[desktop.app]}</strong><span>{!desktop.installed ? "Not installed" : !desktop.running ? "Closed" : desktop.usesManagedProxy || desktop.usesLocalDaemon || desktop.usesDaemonProxy ? "Managed backend" : "Private backend; setup needed"}</span></div>)}</div>}
      <div className="desktopControls"><label><input type="checkbox" checked={sync.settings.enabled} disabled={busy || settingsLocked} onChange={(e) => void perform(() => api.setDesktopSync({ ...sync.settings, enabled: e.target.checked }))} />Apply desktop when selecting an account</label><label><input type="checkbox" checked={sync.settings.resumeActiveSessions !== false} disabled={busy || settingsLocked} onChange={(e) => void perform(() => api.setDesktopSync({ ...sync.settings, resumeActiveSessions: e.target.checked }))} />Restart &amp; continue active chats</label><select aria-label="Desktop app to update" value={sync.settings.app} disabled={busy || settingsLocked} onChange={(e) => void perform(() => api.setDesktopSync({ ...sync.settings, app: e.target.value as DesktopApp }))}>{snapshot.desktops?.filter((d, index, all) => d.installed && all.findIndex((item) => item.app === d.app) === index).map((d) => <option key={`${d.app}:${d.pid}`} value={d.app}>{names[d.app]}</option>)}</select><button disabled={busy || !!pending || !selected || !!selected.apiProvider} onClick={() => void perform(() => api.applyCodexDesktop(sync.settings.app))}><RefreshCw size={13} />Apply desktop</button></div>
      {!compact && <p className="desktopHint">Quota fallback uses healthy subscription accounts. Supported managed desktops restart with the selected credential and continue eligible original chats once. Private desktops need one safe close before their first managed launch. Active approvals and unsupported tools wait for you.</p>}
      {!compact && <LocalChatFinder />}
      {!compact && <div className="desktopCatalogRepair"><button disabled={busy || !!pending} onClick={() => void repair()}><RefreshCw size={13} />Repair older session history</button><small>Repair stored history after work has stopped. Use Find a local chat to reopen ongoing work.</small>{repairResult && <p role="status">{repairResult}</p>}</div>}
      {operation && <details className="desktopOperation"><summary>{phaseName(operation.phase)} · {operation.sessions.length} session(s)</summary><p>{operation.message}</p>{operation.sessions.map((session) => <div className="desktopRecoverySession" key={`${session.threadId}:${session.cwd}`}><strong>{session.name}</strong><span>{phaseName(session.phase)} · {session.message}</span><small>{session.cwd}</small><button onClick={() => void api.openDesktopThread(session.threadId).catch((e) => setError(String(e)))}><ExternalLink size={12} />Open original session</button></div>)}<small>Operation {operation.id} · {formatTime(operation.updatedAt)}</small></details>}
      <small className="desktopQuotaTime">Selected account quota updated: {formatTime(selected?.quota?.updatedAt)}{selected?.quota?.error ? ` · ${selected.quota.error}` : ""}</small>
    </details>
  </section>;
}

function formatTime(value?: string | null) { if (!value) return "Not yet"; const date = new Date(value); return Number.isNaN(date.getTime()) ? "Unavailable" : date.toLocaleTimeString(); }
export function phaseName(phase: string) { return ({ waiting: "Waiting to switch", checkpointing: "Saving state", switching: "Changing account", restoring: "Restoring", retrySwitch: "Retrying handoff", applied: "Applied", continued: "Continued", needsAttention: "Needs attention", interrupted: "Checkpointed for continuation", completed: "Completed", cancelled: "Cancelled", notResumed: "Left for original session" } as Record<string, string>)[phase] ?? phase; }
