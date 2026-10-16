import { useRef, useState } from "react";
import { ExternalLink, Loader2, Search, X } from "lucide-react";
import { api } from "./tauri";
import type { LocalChatSearchResult } from "./types";

export function LocalChatFinder() {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [includeArchived, setIncludeArchived] = useState(false);
  const [result, setResult] = useState<LocalChatSearchResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [opening, setOpening] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const request = useRef(0);
  async function search(text = query, archived = includeArchived) {
    const current = ++request.current;
    setBusy(true); setError(null);
    try {
      const found = await api.findLocalChats(text, archived);
      if (current === request.current) setResult(found);
    } catch (cause) {
      if (current === request.current) { setResult(null); setError(String(cause)); }
    } finally { if (current === request.current) setBusy(false); }
  }
  async function openChat(id: string) {
    setOpening(id); setError(null);
    try { await api.openDesktopThread(id); }
    catch (cause) { setError(String(cause)); }
    finally { setOpening(null); }
  }
  return <div className="localChatFinder">
    <button type="button" aria-expanded={open} onClick={() => {
      setOpen(!open); if (!open) void search();
    }}><Search size={14} />Find a local chat</button>
    {open && <section aria-label="Find a local chat">
      <div className="localChatFinderHeading"><strong>Continue an existing chat</strong>
        <button type="button" aria-label="Close chat finder" onClick={() => setOpen(false)}><X size={14} /></button></div>
      <p>Find chats across your Codex profiles, even while Codex is working. Open the original chat to talk there.</p>
      <form onSubmit={event => { event.preventDefault(); void search(); }}>
        <label><span className="localChatSearchLabel">Title, topic, project folder or chat ID</span>
          <input value={query} onChange={event => setQuery(event.target.value)} maxLength={200} placeholder="Search chats…" autoFocus /></label>
        <button type="submit" disabled={busy}>{busy ? <Loader2 size={14} className="spin" /> : <Search size={14} />}Search</button>
        <label className="localChatArchived"><input type="checkbox" checked={includeArchived} onChange={event => {
          setIncludeArchived(event.target.checked); void search(query, event.target.checked);
        }} />Include archived</label>
      </form>
      {error && <p className="desktopError" role="alert">{error}</p>}
      {busy && <p role="status">Searching local chats…</p>}
      {!busy && result && <>
        <p role="status">{result.chats.length ? `${result.chats.length} chat(s) found` : "No matching local chats."}{result.hasMore ? " · Showing recent matches. Refine your search for older chats." : ""}</p>
        {result.warnings.map(warning => <p className="localChatWarning" key={warning}>{warning}</p>)}
        <ul className="localChatResults">{result.chats.map(chat => <li key={chat.threadId}>
          <div><strong>{chat.name}</strong><small>{chat.cwd || "Project folder unavailable"}</small>
            <small>{new Date(chat.updatedAt * 1000).toLocaleString()} · {chat.profiles.join(", ")}{chat.archived ? " · Archived" : ""}</small></div>
          <button type="button" disabled={opening !== null || !chat.availableInSharedStore} title={chat.availableInSharedStore ? "Open the existing chat" : "This chat exists only in an older profile. Repair its history after active work finishes."} onClick={() => void openChat(chat.threadId)}>
            {opening === chat.threadId ? <Loader2 size={13} className="spin" /> : <ExternalLink size={13} />}Open chat</button>
          {!chat.availableInSharedStore && <small className="localChatRepairNote">Profile-only history · repair after work finishes</small>}
        </li>)}</ul>
      </>}
    </section>}
  </div>;
}
