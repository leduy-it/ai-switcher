import { useState } from "react";
import { Check, Loader2, Upload } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "./tauri";
import type { Account, AppSnapshot, CodexAuthReplacementPreview, CodexAuthSourceInput } from "./types";

export function ReplaceCodexAuthDialog({ account, onClose, onUpdate }: { account: Account; onClose: () => void; onUpdate: (snapshot: AppSnapshot, warning: string | null) => void }) {
  const [mode, setMode] = useState<"file" | "paste">("file");
  const [source, setSource] = useState<CodexAuthSourceInput>({});
  const [preview, setPreview] = useState<CodexAuthReplacementPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const parse = async (value: CodexAuthSourceInput) => {
    setSource(value); setPreview(null); setError(null); setBusy(true);
    try { setPreview(await api.previewCodexAuthReplacement({ accountId: account.id, ...value })); }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  const chooseFile = async () => {
    setBusy(true); setError(null); setPreview(null);
    try {
      const path = await open({ multiple: false, filters: [{ name: "Codex auth.json", extensions: ["json"] }] });
      if (typeof path === "string") await parse({ authFilePath: path });
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  const submit = async () => {
    if (!preview || busy) return;
    setBusy(true); setError(null);
    try { const result=await api.replaceCodexAuth({ accountId: account.id, ...source, previewFingerprint: preview.previewFingerprint }); onUpdate(result.snapshot,result.warning); onClose(); }
    catch (e) { setError(String(e)); setPreview(null); }
    finally { setBusy(false); }
  };
  return <div className="modalBackdrop" role="presentation" onMouseDown={() => !busy && onClose()}>
    <section className="modal" role="dialog" aria-modal="true" aria-labelledby="replaceCodexAuthTitle" onMouseDown={(event) => event.stopPropagation()}>
      <h2 id="replaceCodexAuthTitle">Import new Codex auth</h2>
      <p>Keep <strong>{account.name}</strong>, its command, settings and chat history. Replace only this profile’s auth.json with the JSON you choose.</p>
      <div className="kindToggle" aria-label="Credential source">{(["file", "paste"] as const).map((value) => <button key={value} disabled={busy} className={mode === value ? "active" : ""} onClick={() => { setMode(value); setSource({}); setPreview(null); setError(null); }}>{value === "file" ? "Import file" : "Paste JSON"}</button>)}</div>
      {mode === "file" ? <button className="fetchModels" disabled={busy} onClick={() => void chooseFile()}><Upload size={14} />Choose auth.json</button> : <>
        <label>Codex auth.json<textarea className="authJsonBox" rows={7} value={source.authJson ?? ""} maxLength={1024 * 1024} disabled={busy} autoComplete="off" autoCapitalize="off" autoCorrect="off" spellCheck={false} onChange={(event) => { setSource({ authJson: event.target.value }); setPreview(null); setError(null); }} /></label>
        <button className="fetchModels" disabled={busy || !source.authJson?.trim()} onClick={() => void parse(source)}><Check size={14} />Preview JSON</button>
      </>}
      {preview && <div className="authJsonPreview" role="status"><span>Current: {preview.currentEmail ?? account.accountEmail ?? "Email unavailable"}</span><strong>Imported: {preview.incomingEmail ?? "Email unavailable in JSON"}</strong><small>Profile and account count stay unchanged. The previous auth is backed up privately. Importing JSON does not renew tokens; quota refresh checks the imported login.</small></div>}
      <p className="hint">Finish or cancel its pending desktop switch, then close Codex desktop/CLI clients using this profile first. Other profiles are preserved. List imports still append new accounts.</p>
      {error && <p className="error" role="alert">{error}</p>}
      <div className="modalActions"><button disabled={busy} onClick={onClose}>Cancel</button><button className="primary" disabled={busy || !preview} onClick={() => void submit()}>{busy ? <Loader2 size={14} className="spin" /> : <Upload size={14} />}Import into this profile</button></div>
    </section>
  </div>;
}
