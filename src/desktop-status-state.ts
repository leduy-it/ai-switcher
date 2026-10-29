import type { AppSnapshot } from "./types";

/** Metadata only. A saved email is proof only while that exact native backend is still running. */
export function desktopStatusState(snapshot: AppSnapshot) {
  const sync = snapshot.desktopSync;
  const tool = snapshot.tools.find((t) => t.id === "codex");
  const selected = tool?.accounts.find((a) => a.id === tool.activeAccountId);
  const live = (snapshot.desktops ?? []).filter((d) => d.running);
  const matching = live.find((d) => d.app === sync?.settings.app && d.profileHome === snapshot.selectedCodexHome && d.sessionHome === snapshot.sharedCodexHome);
  const proof = live.find((d) => d.app === sync?.settings.app && d.backendPid != null && d.backendPid === sync.confirmedBackendPid);
  const actualEmail = proof && sync?.confirmedAt && !sync.error ? sync.confirmedEmail : null;
  const confirmedEmail = matching && actualEmail && sync?.lastAppliedAccountId === selected?.id && actualEmail.toLowerCase() === selected?.accountEmail?.toLowerCase() ? actualEmail : null;
  const op = sync?.operation;
  const terminal = !op || ["applied", "cancelled"].includes(op.phase);
  const protectedSessions = op?.sessions.some((s) => ["checkpointing", "interrupted", "restoring"].includes(s.phase));
  const settingsLocked = !terminal && (!!protectedSessions || ["checkpointing", "switching", "restoring", "retrySwitch"].includes(op!.phase) || op!.blockReason === "nativeContinuation");
  const privateBackend = live.some((d) => !d.backendPid || !d.profileHome || !(d.usesManagedProxy || d.usesLocalDaemon || d.usesDaemonProxy));
  const canSwitchNow = op?.phase === "waiting" && !["otherClient", "pendingTool"].includes(op.blockReason ?? "") && !settingsLocked && !privateBackend && !!selected && !selected.apiProvider;
  return { selected, matching, confirmedEmail, actualEmail, settingsLocked, privateBackend, canSwitchNow, pending: !terminal };
}
