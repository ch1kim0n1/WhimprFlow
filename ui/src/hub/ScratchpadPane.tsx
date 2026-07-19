import { useEffect, useState } from "react";
import type { CSSProperties, Dispatch, SetStateAction } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Button, Card, PageTitle, Segmented } from "./ui";
import { Icon } from "./icons";
import { dayKey, dayLabel, fmtTimeOfDay } from "./format";
import {
  addNote,
  captureScreen,
  getHistory,
  getNotes,
  removeNote,
  runTransform,
  type HistoryItem,
  type Note,
} from "./api";

const STORAGE_KEY = "whimpr:scratchpad";

// The canned instruction handed to the cleanup provider by the Polish button.
const POLISH_INSTRUCTION =
  "Clean up grammar, filler and punctuation. Preserve meaning, facts, names, numbers and formatting.";

// The canned instruction handed to the cleanup provider by the Compile button:
// restructure into a typed draft, then flag every fact the draft still needs.
const COMPILE_INSTRUCTION =
  "Restructure this dictation into a typed draft. Detect the intent (email, spec, issue, or note) " +
  "and format it that way with clear section headings. Keep every stated fact, name and number; " +
  "do not invent content. After the draft, output one line per fact that is needed but was not " +
  "stated, each on its own line starting with exactly 'MISSING: '. Output only the draft followed " +
  "by the MISSING lines.";

// Tolerates a stray bullet prefix in case the model lists the MISSING lines.
const MISSING_RE = /^\s*[-*]?\s*MISSING:\s*/;

type StudioTab = "editor" | "timeline" | "notes";

const TABS: { value: StudioTab; label: string }[] = [
  { value: "editor", label: "Editor" },
  { value: "timeline", label: "Timeline" },
  { value: "notes", label: "Notes" },
];

const ICON_BTN: CSSProperties = {
  border: "none",
  background: "transparent",
  cursor: "pointer",
  color: theme.textFaint,
  display: "flex",
  alignItems: "center",
  padding: 4,
};

function loadInitial(): string {
  try {
    return localStorage.getItem(STORAGE_KEY) ?? "";
  } catch {
    // localStorage can throw in some webview contexts; never crash the pane.
    return "";
  }
}

// First line becomes the title; the rest is the body.
function splitTitleBody(text: string): { title: string; body: string } {
  const trimmed = text.trim();
  const nl = trimmed.indexOf("\n");
  if (nl === -1) return { title: trimmed, body: "" };
  return { title: trimmed.slice(0, nl).trim(), body: trimmed.slice(nl + 1).trim() };
}

function asGithubIssue(text: string): string {
  const { title, body } = splitTitleBody(text);
  return `# ${title}\n\n### Summary\n\n${body || title}`;
}

function asLinear(text: string): string {
  const { title, body } = splitTitleBody(text);
  return `**${title}**\n\n${body || title}`;
}

// True when every char of q appears in order within hay (simple fuzzy).
function isSubsequence(q: string, hay: string): boolean {
  let i = 0;
  for (let j = 0; j < hay.length && i < q.length; j++) {
    if (hay[j] === q[i]) i++;
  }
  return i === q.length;
}

export function ScratchpadPane() {
  const [tab, setTab] = useState<StudioTab>("editor");
  const [text, setText] = useState<string>(loadInitial);
  // Compile's flagged facts live here so the card survives tab switches.
  const [missing, setMissing] = useState<string[]>([]);

  // Persist on every change. Guarded so a storage failure never crashes the pane.
  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, text);
    } catch {
      /* localStorage unavailable in this webview; persistence is best-effort */
    }
  }, [text]);

  // Timeline "Insert" appends into the editor content with a blank-line gap.
  const insertIntoEditor = (t: string) =>
    setText((prev) => (prev.trim() ? `${prev.replace(/\s+$/, "")}\n\n${t}` : t));

  return (
    <div style={{ maxWidth: 820 }}>
      <PageTitle sub="Draft long-form by voice, revisit every dictation on the timeline, and keep captured notes in one place. Everything is saved locally on this machine.">
        Voice Studio
      </PageTitle>

      <div style={{ marginBottom: 16 }}>
        <Segmented options={TABS} value={tab} onChange={setTab} />
      </div>

      {tab === "editor" && (
        <EditorTab text={text} setText={setText} missing={missing} setMissing={setMissing} />
      )}
      {tab === "timeline" && <TimelineTab onInsert={insertIntoEditor} />}
      {tab === "notes" && <NotesTab />}
    </div>
  );
}

// ── Editor ────────────────────────────────────────────────────────────────────
function EditorTab({
  text,
  setText,
  missing,
  setMissing,
}: {
  text: string;
  setText: Dispatch<SetStateAction<string>>;
  missing: string[];
  setMissing: Dispatch<SetStateAction<string[]>>;
}) {
  const [busy, setBusy] = useState<null | "polish" | "compile">(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState<null | "plain" | "github" | "linear">(null);
  const [confirmClear, setConfirmClear] = useState(false);

  // Auto-clear the "Copied" flash.
  useEffect(() => {
    if (!copied) return;
    const id = setTimeout(() => setCopied(null), 1600);
    return () => clearTimeout(id);
  }, [copied]);

  // A stray click on Clear only arms the confirm state; it disarms itself.
  useEffect(() => {
    if (!confirmClear) return;
    const id = setTimeout(() => setConfirmClear(false), 3000);
    return () => clearTimeout(id);
  }, [confirmClear]);

  const empty = text.trim().length === 0;
  const words = text.trim() ? text.trim().split(/\s+/).length : 0;
  const chars = text.length;

  const copyAs = (what: "plain" | "github" | "linear") => {
    const content = what === "github" ? asGithubIssue(text) : what === "linear" ? asLinear(text) : text;
    navigator.clipboard.writeText(content).then(
      () => setCopied(what),
      () => {
        /* clipboard blocked; leave the button label unchanged */
      },
    );
  };

  const onExportMd = () => {
    const blob = new Blob([text], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "whimpr-studio.md";
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
  };

  const onPolish = async () => {
    setBusy("polish");
    setError(null);
    try {
      const polished = await runTransform(text, POLISH_INSTRUCTION);
      setText(polished);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const onCompile = async () => {
    setBusy("compile");
    setError(null);
    try {
      const out = await runTransform(text, COMPILE_INSTRUCTION);
      // Split the MISSING lines out of the draft; the rest goes into the editor.
      const facts: string[] = [];
      const draft: string[] = [];
      for (const line of out.split("\n")) {
        const m = line.match(MISSING_RE);
        if (m) {
          const fact = line.slice(m[0].length).trim();
          if (fact) facts.push(fact);
        } else {
          draft.push(line);
        }
      }
      setMissing(facts);
      setText(draft.join("\n").trim());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const onClear = () => {
    if (!confirmClear) {
      setConfirmClear(true);
      return;
    }
    setText("");
    setConfirmClear(false);
  };

  return (
    <div>
      {missing.length > 0 && (
        <Card pad={14} style={{ marginBottom: 12, borderColor: theme.accentSoftBorder }}>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 8 }}>
            <div style={{ fontSize: 13, fontWeight: 600, color: theme.textStrong }}>Missing facts</div>
            <button aria-label="Dismiss missing facts" title="Dismiss" onClick={() => setMissing([])} style={ICON_BTN}>
              <Icon name="close" size={15} />
            </button>
          </div>
          <ul style={{ margin: 0, padding: "0 0 0 18px", display: "flex", flexDirection: "column", gap: 4 }}>
            {missing.map((m, i) => (
              <li key={i} style={{ fontSize: 13, lineHeight: 1.5, color: theme.textBody }}>
                {m}
              </li>
            ))}
          </ul>
        </Card>
      )}

      <Card pad={14} style={{ marginBottom: 12 }}>
        <textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="Start typing, or focus here and hold Fn to dictate..."
          style={{
            width: "100%",
            minHeight: 340,
            background: theme.cardBgSubtle,
            border: `1px solid ${theme.border}`,
            borderRadius: 12,
            padding: 14,
            color: theme.textBody,
            fontFamily: font.ui,
            fontSize: 14,
            lineHeight: 1.6,
            boxSizing: "border-box",
            resize: "vertical",
            outline: "none",
          }}
        />
      </Card>

      <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
        <div style={{ fontSize: 12.5, color: theme.textMuted }}>
          {words} {words === 1 ? "word" : "words"} &middot; {chars} {chars === 1 ? "char" : "chars"}
        </div>
        <div style={{ flex: 1 }} />
        <Button variant="ghost" onClick={() => copyAs("plain")} disabled={empty}>
          {copied === "plain" ? "Copied" : "Copy"}
        </Button>
        <span title="Polish is macOS-only; it may reject on other platforms.">
          <Button variant="accent" onClick={onPolish} disabled={busy !== null || empty}>
            {busy === "polish" ? "Polishing..." : "Polish"}
          </Button>
        </span>
        <span title="Compile is macOS-only; it may reject on other platforms.">
          <Button variant="accent" onClick={onCompile} disabled={busy !== null || empty}>
            {busy === "compile" ? "Compiling..." : "Compile"}
          </Button>
        </span>
        <Button variant="ghost" onClick={onClear} disabled={empty}>
          {confirmClear ? "Really clear?" : "Clear"}
        </Button>
      </div>

      {/* Export row: hand the draft to a file or an issue tracker. */}
      <div style={{ display: "flex", alignItems: "center", gap: 8, marginTop: 10, flexWrap: "wrap" }}>
        <Button variant="ghost" size="sm" onClick={onExportMd} disabled={empty}>
          Export .md
        </Button>
        <Button variant="ghost" size="sm" onClick={() => copyAs("github")} disabled={empty}>
          {copied === "github" ? "Copied" : "Copy as GitHub issue"}
        </Button>
        <Button variant="ghost" size="sm" onClick={() => copyAs("linear")} disabled={empty}>
          {copied === "linear" ? "Copied" : "Copy as Linear"}
        </Button>
      </div>

      <div style={{ fontSize: 12, color: theme.textMuted, marginTop: 8 }}>
        Polish cleans up grammar and filler. Compile restructures the text into a typed draft and
        flags missing facts. Both run through your configured cleanup provider; macOS-only for now.
      </div>

      {error && <div style={{ fontSize: 12.5, color: "#e5484d", marginTop: 8 }}>{error}</div>}
    </div>
  );
}

// ── Timeline ──────────────────────────────────────────────────────────────────
type Group = { key: string; label: string; items: HistoryItem[] };
function groupByDay(items: HistoryItem[]): Group[] {
  const now = new Date();
  const groups: Group[] = [];
  const index = new Map<string, Group>();
  for (const it of items) {
    const d = new Date(it.ts_unix * 1000);
    const k = dayKey(d);
    let g = index.get(k);
    if (!g) {
      g = { key: k, label: dayLabel(d, now), items: [] };
      index.set(k, g);
      groups.push(g);
    }
    g.items.push(it);
  }
  return groups;
}

// Timeline search covers the full history, not the default newest-200 page.
// The backend caps rows with take(limit), so any value beyond the total row
// count means "all"; no dictation history realistically outgrows this.
const FULL_HISTORY_LIMIT = 1_000_000;

function TimelineTab({ onInsert }: { onInsert: (t: string) => void }) {
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [query, setQuery] = useState("");

  useEffect(() => {
    let alive = true;
    getHistory(FULL_HISTORY_LIMIT).then((h) => alive && setHistory(h));
    return () => {
      alive = false;
    };
  }, []);

  // Substring across raw + final first; when nothing hits, fall back to a
  // simple in-order subsequence match so "mtg nts" still finds "meeting notes".
  // ponytail: substring + subsequence only; retrieval v2 adds embeddings and a
  // voice query loop.
  const q = query.trim().toLowerCase();
  let filtered = history;
  if (q) {
    const inItem = (it: HistoryItem, match: (hay: string) => boolean) =>
      match(it.text.toLowerCase()) || match(it.raw.toLowerCase());
    const substr = history.filter((it) => inItem(it, (hay) => hay.includes(q)));
    filtered = substr.length > 0 ? substr : history.filter((it) => inItem(it, (hay) => isSubsequence(q, hay)));
  }
  const groups = groupByDay(filtered);

  return (
    <Card pad={0}>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "flex-end", padding: "14px 20px", borderBottom: `1px solid ${theme.border}` }}>
        <div style={{ display: "flex", alignItems: "center", gap: 7, background: theme.cardBgSubtle, border: `1px solid ${theme.border}`, borderRadius: 9, padding: "6px 10px", minWidth: 220 }}>
          <Icon name="search" size={15} style={{ color: theme.textFaint }} />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search raw and final text"
            aria-label="Search dictation history"
            style={{ border: "none", outline: "none", background: "transparent", fontFamily: font.ui, fontSize: 13, color: theme.textBody, width: "100%" }}
          />
        </div>
      </div>
      <div style={{ padding: "4px 20px 16px" }}>
        {history.length === 0 ? (
          <div style={{ padding: "40px 8px", textAlign: "center", color: theme.textFaint, fontSize: 13.5 }}>
            No dictations yet. Hold Fn and speak to add your first.
          </div>
        ) : filtered.length === 0 ? (
          <div style={{ padding: "40px 8px", textAlign: "center", color: theme.textFaint, fontSize: 13.5 }}>
            No dictations match "{query}".
          </div>
        ) : (
          groups.map((g) => (
            <div key={g.key} style={{ marginTop: 14 }}>
              <div style={{ fontSize: 11, fontWeight: 700, letterSpacing: 0.6, textTransform: "uppercase", color: theme.accentDeep, marginBottom: 2 }}>
                {g.label}
              </div>
              {g.items.map((it, i) => (
                <TimelineRow key={`${it.ts_unix}-${i}`} it={it} onInsert={onInsert} />
              ))}
            </div>
          ))
        )}
      </div>
    </Card>
  );
}

function RawFinalToggle({ raw, onChange }: { raw: boolean; onChange: (r: boolean) => void }) {
  const opts = [
    { v: false, label: "Final" },
    { v: true, label: "Raw" },
  ];
  return (
    <div style={{ display: "inline-flex", gap: 2, background: theme.track, borderRadius: 8, padding: 2 }}>
      {opts.map((o) => {
        const active = raw === o.v;
        return (
          <button
            key={o.label}
            onClick={() => onChange(o.v)}
            style={{
              border: "none",
              cursor: "pointer",
              borderRadius: 6,
              padding: "3px 9px",
              fontSize: 11.5,
              fontFamily: font.ui,
              fontWeight: active ? 600 : 500,
              color: active ? theme.accentDeep : theme.textMuted,
              background: active ? theme.cardBg : "transparent",
            }}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

function TimelineRow({ it, onInsert }: { it: HistoryItem; onInsert: (t: string) => void }) {
  const [showRaw, setShowRaw] = useState(false);
  const [inserted, setInserted] = useState(false);
  // Only offer the toggle when there is a raw transcript that actually differs.
  const hasRaw = it.raw.trim().length > 0 && it.raw !== it.text;
  const shown = hasRaw && showRaw ? it.raw : it.text;

  // Auto-clear the "Inserted" flash (the editor lives on another tab).
  useEffect(() => {
    if (!inserted) return;
    const id = setTimeout(() => setInserted(false), 1600);
    return () => clearTimeout(id);
  }, [inserted]);

  return (
    <div style={{ display: "flex", gap: 14, padding: "11px 4px", borderBottom: `1px solid ${theme.border}` }}>
      <div style={{ flex: "0 0 64px", fontSize: 12, color: theme.textFaint, fontVariantNumeric: "tabular-nums", paddingTop: 1 }}>
        {fmtTimeOfDay(new Date(it.ts_unix * 1000))}
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 13.5, lineHeight: 1.5, color: hasRaw && showRaw ? theme.textMuted : theme.textBody, whiteSpace: "pre-wrap" }}>
          {shown}
        </div>
        <div style={{ fontSize: 11, color: theme.textFaint, marginTop: 3 }}>
          {it.app ? `${it.app} · ` : ""}
          {it.words} {it.words === 1 ? "word" : "words"}
        </div>
      </div>
      <div style={{ display: "flex", flexDirection: "column", alignItems: "flex-end", gap: 6, flex: "0 0 auto" }}>
        {hasRaw && <RawFinalToggle raw={showRaw} onChange={setShowRaw} />}
        <Button
          variant="ghost"
          size="sm"
          onClick={() => {
            onInsert(shown);
            setInserted(true);
          }}
        >
          {inserted ? "Inserted" : "Insert"}
        </Button>
      </div>
    </div>
  );
}

// ── Notes ─────────────────────────────────────────────────────────────────────
function NotesTab() {
  const [notes, setNotes] = useState<Note[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [snapping, setSnapping] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    const n = await getNotes();
    setNotes(n);
    setLoaded(true);
  };
  useEffect(() => {
    void load();
  }, []);

  const onSnap = async () => {
    setSnapping(true);
    setError(null);
    try {
      const path = await captureScreen();
      await addNote("Screen capture", "", path);
      await load();
    } catch (e) {
      // capture_screen is macOS-only and can fail on missing screen-recording
      // permission; show the shell's message instead of crashing the pane.
      setError(String(e));
    } finally {
      setSnapping(false);
    }
  };

  const remove = async (ts: number) => {
    await removeNote(ts);
    await load();
  };

  return (
    <div>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12, marginBottom: 14 }}>
        <div style={{ fontSize: 12.5, color: theme.textMuted }}>
          Meeting transcripts, workflow outputs and screen captures land here.
        </div>
        <span title="Screen capture is macOS-only.">
          <Button variant="accent" size="sm" onClick={() => void onSnap()} disabled={snapping}>
            {snapping ? "Capturing..." : "Snap screen + note"}
          </Button>
        </span>
      </div>

      {error && <div style={{ fontSize: 12.5, color: "#e5484d", marginBottom: 10 }}>{error}</div>}

      <Card pad={notes.length ? 8 : 22}>
        {notes.length === 0 ? (
          <div style={{ padding: "36px 8px", textAlign: "center", color: theme.textFaint, fontSize: 13.5 }}>
            {loaded ? "No notes yet." : "Loading notes..."}
          </div>
        ) : (
          <div style={{ padding: "4px 14px" }}>
            {notes.map((n) => (
              <NoteRow key={n.ts_unix} note={n} onRemove={() => void remove(n.ts_unix)} />
            ))}
          </div>
        )}
      </Card>
    </div>
  );
}

function NoteRow({ note, onRemove }: { note: Note; onRemove: () => void }) {
  const [hover, setHover] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [pathCopied, setPathCopied] = useState(false);
  const d = new Date(note.ts_unix * 1000);
  // Collapse long notes to roughly six lines until expanded.
  const long = note.text.split("\n").length > 6 || note.text.length > 480;

  useEffect(() => {
    if (!pathCopied) return;
    const id = setTimeout(() => setPathCopied(false), 1600);
    return () => clearTimeout(id);
  }, [pathCopied]);

  const copyPath = () => {
    if (!note.image_path) return;
    navigator.clipboard.writeText(note.image_path).then(
      () => setPathCopied(true),
      () => {
        /* clipboard blocked; leave the button label unchanged */
      },
    );
  };

  return (
    <div
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{ padding: "13px 6px", borderBottom: `1px solid ${theme.border}` }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>{note.title}</span>
        <span style={{ fontSize: 12, color: theme.textFaint, fontVariantNumeric: "tabular-nums" }}>
          {dayLabel(d)} {"·"} {fmtTimeOfDay(d)}
        </span>
        <div style={{ flex: 1 }} />
        <button
          onClick={onRemove}
          aria-label="Delete note"
          title="Delete note"
          style={{ ...ICON_BTN, opacity: hover ? 1 : 0, transition: "opacity 120ms ease" }}
        >
          <Icon name="close" size={16} />
        </button>
      </div>

      {note.text.trim().length > 0 && (
        <>
          <div
            style={{
              marginTop: 6,
              fontSize: 13.5,
              lineHeight: 1.55,
              color: theme.textBody,
              whiteSpace: "pre-wrap",
              ...(long && !expanded
                ? { display: "-webkit-box", WebkitLineClamp: 6, WebkitBoxOrient: "vertical" as const, overflow: "hidden" }
                : {}),
            }}
          >
            {note.text}
          </div>
          {long && (
            <button
              onClick={() => setExpanded((e) => !e)}
              style={{
                border: "none",
                background: "transparent",
                cursor: "pointer",
                padding: 0,
                marginTop: 4,
                fontFamily: font.ui,
                fontSize: 12,
                fontWeight: 600,
                color: theme.accentDeep,
              }}
            >
              {expanded ? "Show less" : "Show more"}
            </button>
          )}
        </>
      )}

      {/* ponytail: capture shown as its file path only, no inline preview;
          rendering the image needs the asset protocol scoped to captures/. */}
      {note.image_path && (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 6,
            marginTop: 8,
            background: theme.cardBgSubtle,
            border: `1px solid ${theme.border}`,
            borderRadius: 8,
            padding: "5px 8px",
          }}
        >
          <code
            style={{
              fontFamily: font.mono,
              fontSize: 11.5,
              color: theme.textMuted,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              flex: 1,
            }}
          >
            {note.image_path}
          </code>
          <button
            onClick={copyPath}
            style={{
              border: "none",
              background: "transparent",
              cursor: "pointer",
              padding: "1px 4px",
              fontFamily: font.ui,
              fontSize: 11.5,
              fontWeight: 600,
              color: pathCopied ? theme.accentDeep : theme.textMuted,
            }}
          >
            {pathCopied ? "Copied" : "Copy"}
          </button>
        </div>
      )}
    </div>
  );
}
