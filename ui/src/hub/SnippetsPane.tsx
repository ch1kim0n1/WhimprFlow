import { useEffect, useState } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Button, Card } from "./ui";
import { Icon } from "./icons";
import { addSnippet, getSnippets, removeSnippet, type SnippetEntry } from "./api";

function AddForm({ onDone }: { onDone: () => void }) {
  const [trigger, setTrigger] = useState("");
  const [expansion, setExpansion] = useState("");
  const inputStyle = {
    width: "100%",
    background: theme.cardBgSubtle,
    border: `1px solid ${theme.border}`,
    borderRadius: 10,
    padding: "9px 12px",
    color: theme.textBody,
    fontFamily: font.ui,
    fontSize: 13.5,
    outline: "none",
    boxSizing: "border-box" as const,
    resize: "vertical" as const,
  } as const;

  const submit = async () => {
    const t = trigger.trim();
    const x = expansion.trim();
    if (!t || !x) return;
    await addSnippet(t, x);
    onDone();
  };

  return (
    <Card style={{ marginBottom: 16, borderColor: theme.accentSoftBorder }}>
      <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong, marginBottom: 12 }}>Add a snippet</div>
      <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        <div>
          <label style={{ fontSize: 12, color: theme.textMuted, display: "block", marginBottom: 5 }}>
            Trigger phrase
          </label>
          <input
            autoFocus
            value={trigger}
            onChange={(e) => setTrigger(e.target.value)}
            placeholder="e.g. my address"
            style={inputStyle}
            onKeyDown={(e) => {
              if (e.key === "Enter") void submit();
            }}
          />
        </div>
        <div>
          <label style={{ fontSize: 12, color: theme.textMuted, display: "block", marginBottom: 5 }}>
            Expands to
          </label>
          <textarea
            value={expansion}
            onChange={(e) => setExpansion(e.target.value)}
            placeholder="123 Main St, Springfield"
            rows={3}
            style={inputStyle}
          />
        </div>
      </div>
      <div style={{ display: "flex", gap: 8, marginTop: 14 }}>
        <Button variant="accent" onClick={() => void submit()}>
          Add snippet
        </Button>
        <Button variant="ghost" onClick={onDone}>
          Cancel
        </Button>
      </div>
    </Card>
  );
}

function EntryRow({ entry, onRemove }: { entry: SnippetEntry; onRemove: () => void }) {
  const [hover, setHover] = useState(false);
  return (
    <div
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        display: "flex",
        alignItems: "flex-start",
        justifyContent: "space-between",
        gap: 12,
        padding: "12px 6px",
        borderBottom: `1px solid ${theme.border}`,
      }}
    >
      <div style={{ minWidth: 0 }}>
        <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>{entry.trigger}</div>
        <div style={{ marginTop: 3, fontSize: 12.5, color: theme.textMuted, whiteSpace: "pre-wrap" }}>
          {entry.expansion}
        </div>
      </div>
      <button
        onClick={onRemove}
        title="Remove"
        style={{
          border: "none",
          background: "transparent",
          cursor: "pointer",
          color: theme.textFaint,
          opacity: hover ? 1 : 0,
          transition: "opacity 120ms ease",
          display: "flex",
          alignItems: "center",
          padding: 4,
          flex: "0 0 auto",
        }}
      >
        <Icon name="close" size={16} />
      </button>
    </div>
  );
}

export function SnippetsPane() {
  const [entries, setEntries] = useState<SnippetEntry[]>([]);
  const [query, setQuery] = useState("");
  const [adding, setAdding] = useState(false);

  const load = () => getSnippets().then(setEntries);
  useEffect(() => {
    void load();
  }, []);

  const remove = async (trigger: string) => {
    await removeSnippet(trigger);
    await load();
  };

  const q = query.trim().toLowerCase();
  const filtered = q
    ? entries.filter(
        (e) => e.trigger.toLowerCase().includes(q) || e.expansion.toLowerCase().includes(q),
      )
    : entries;

  return (
    <div style={{ maxWidth: 760 }}>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 18 }}>
        <div>
          <h1
            style={{
              fontFamily: font.serif,
              fontSize: 30,
              fontWeight: 600,
              letterSpacing: -0.4,
              margin: 0,
              color: theme.textStrong,
            }}
          >
            Snippets
          </h1>
          <p style={{ color: theme.textMuted, fontSize: 14, margin: "8px 0 0" }}>
            Say a short trigger and WhimprFlow pastes the full text: signatures, addresses, boilerplate.
          </p>
        </div>
        <Button variant="accent" onClick={() => setAdding((a) => !a)}>
          <Icon name="plus" size={15} style={{ color: "#fff" }} />
          Add new
        </Button>
      </div>

      <div style={{ display: "flex", alignItems: "center", justifyContent: "flex-end", gap: 10, marginBottom: 14 }}>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 7,
            background: theme.cardBg,
            border: `1px solid ${theme.border}`,
            borderRadius: 9,
            padding: "6px 10px",
            minWidth: 200,
          }}
        >
          <Icon name="search" size={15} style={{ color: theme.textFaint }} />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search snippets"
            style={{
              border: "none",
              outline: "none",
              background: "transparent",
              fontFamily: font.ui,
              fontSize: 13,
              color: theme.textBody,
              width: "100%",
            }}
          />
        </div>
      </div>

      {adding && (
        <AddForm
          onDone={() => {
            setAdding(false);
            void load();
          }}
        />
      )}

      <Card pad={filtered.length ? 8 : 22}>
        {filtered.length === 0 ? (
          <div style={{ padding: "30px 8px", textAlign: "center", color: theme.textFaint, fontSize: 13.5 }}>
            {entries.length === 0
              ? "No snippets yet. Add one to expand a short spoken trigger into a full phrase."
              : `No snippets match "${query}".`}
          </div>
        ) : (
          <div style={{ padding: "4px 14px" }}>
            {filtered.map((e) => (
              <EntryRow key={e.trigger} entry={e} onRemove={() => void remove(e.trigger)} />
            ))}
          </div>
        )}
      </Card>
    </div>
  );
}
