import { useEffect, useState } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Button, Card, PageTitle } from "./ui";
import { runTransform } from "./api";

const STORAGE_KEY = "whimpr:scratchpad";

// The canned instruction handed to the cleanup provider by the Polish button.
const POLISH_INSTRUCTION =
  "Clean up grammar, filler and punctuation. Preserve meaning, facts, names, numbers and formatting.";

function loadInitial(): string {
  try {
    return localStorage.getItem(STORAGE_KEY) ?? "";
  } catch {
    // localStorage can throw in some webview contexts; never crash the pane.
    return "";
  }
}

export function ScratchpadPane() {
  const [text, setText] = useState<string>(loadInitial);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);

  // Persist on every change. Guarded so a storage failure never crashes the pane.
  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, text);
    } catch {
      /* localStorage unavailable in this webview; persistence is best-effort */
    }
  }, [text]);

  // Auto-clear the "Copied" flash.
  useEffect(() => {
    if (!copied) return;
    const id = setTimeout(() => setCopied(false), 1600);
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

  const onCopy = () => {
    navigator.clipboard.writeText(text).then(
      () => setCopied(true),
      () => {
        /* clipboard blocked; leave the button label unchanged */
      },
    );
  };

  const onPolish = async () => {
    setBusy(true);
    setError(null);
    try {
      const polished = await runTransform(text, POLISH_INSTRUCTION);
      setText(polished);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
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
    <div style={{ maxWidth: 820 }}>
      <PageTitle sub="A quiet place to dictate long-form and shape it before it goes anywhere. Focus the box, hold Fn, and speak; your words land here. Everything is saved locally and stays on this machine.">
        Scratchpad
      </PageTitle>

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
        <Button variant="ghost" onClick={onCopy} disabled={empty}>
          {copied ? "Copied" : "Copy"}
        </Button>
        <span title="Polish is macOS-only; it may reject on other platforms.">
          <Button variant="accent" onClick={onPolish} disabled={busy || empty}>
            {busy ? "Polishing..." : "Polish"}
          </Button>
        </span>
        <Button variant="ghost" onClick={onClear} disabled={empty}>
          {confirmClear ? "Really clear?" : "Clear"}
        </Button>
      </div>

      <div style={{ fontSize: 12, color: theme.textMuted, marginTop: 8 }}>
        Polish cleans up grammar and filler through your configured cleanup provider. macOS-only for now.
      </div>

      {error && <div style={{ fontSize: 12.5, color: "#e5484d", marginTop: 8 }}>{error}</div>}
    </div>
  );
}
