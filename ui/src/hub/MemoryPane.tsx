import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { font, palette } from "../tokens/values";
import { theme } from "./theme";
import { Button, Card, PageTitle } from "./ui";
import { Icon } from "./icons";
import { dayLabel, fmtTimeOfDay } from "./format";
import {
  clearVoiceMemory,
  exportVoiceMemory,
  getDictionary,
  getVoiceMemory,
  type CorrectionEvent,
} from "./api";

// Voice Memory pane: the auditable view over the encrypted local learning log.
// Shows every learned correction, summarizes auto-learned vocabulary, exports
// the whole bundle as plain JSON, and clears the log behind a two-step confirm.

// "Today, 9:41 am" / "Jul 15, 9:41 am" for a correction's timestamp.
function whenLabel(tsUnix: number): string {
  const d = new Date(tsUnix * 1000);
  return `${dayLabel(d)}, ${fmtTimeOfDay(d)}`;
}

// Friendly names for the known correction sources; unknown sources pass through.
function sourceLabel(source: string): string {
  if (source === "autolearn") return "auto-learned";
  if (source === "manual") return "manual edit";
  return source;
}

function SectionTitle({ children }: { children: ReactNode }) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 7,
        fontSize: 14,
        fontWeight: 600,
        color: theme.textStrong,
        marginBottom: 10,
      }}
    >
      {children}
    </div>
  );
}

function CorrectionRow({ ev }: { ev: CorrectionEvent }) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "baseline",
        gap: 12,
        padding: "11px 0",
        borderBottom: `1px solid ${theme.border}`,
      }}
    >
      <span style={{ flex: "0 0 132px", fontSize: 12, color: theme.textFaint }}>{whenLabel(ev.ts_unix)}</span>
      <span style={{ flex: 1, minWidth: 0, fontSize: 13.5, overflowWrap: "anywhere" }}>
        <span style={{ color: theme.textMuted, textDecoration: "line-through" }}>{ev.from}</span>
        <span style={{ color: theme.textFaint, margin: "0 8px" }}>→</span>
        <span style={{ color: theme.textStrong, fontWeight: 600 }}>{ev.to}</span>
      </span>
      <span
        style={{
          flex: "0 0 auto",
          fontSize: 11,
          fontWeight: 600,
          color: theme.textMuted,
          background: theme.track,
          borderRadius: 999,
          padding: "2px 8px",
        }}
      >
        {sourceLabel(ev.source)}
      </span>
    </div>
  );
}

export function MemoryPane() {
  const [corrections, setCorrections] = useState<CorrectionEvent[]>([]);
  const [autoWords, setAutoWords] = useState<string[]>([]);
  const [exporting, setExporting] = useState(false);
  const [exportPath, setExportPath] = useState<string | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [confirmingClear, setConfirmingClear] = useState(false);

  const load = () =>
    Promise.all([getVoiceMemory(), getDictionary()]).then(([mem, dict]) => {
      setCorrections([...mem].sort((a, b) => b.ts_unix - a.ts_unix));
      setAutoWords(dict.filter((e) => e.auto).map((e) => e.correct));
    });
  useEffect(() => {
    void load();
  }, []);

  const doExport = async () => {
    setExporting(true);
    setExportError(null);
    try {
      setExportPath(await exportVoiceMemory());
      setCopied(false);
    } catch (e) {
      setExportPath(null);
      setExportError(e instanceof Error ? e.message : String(e));
    } finally {
      setExporting(false);
    }
  };

  const copyPath = () => {
    if (!exportPath) return;
    navigator.clipboard.writeText(exportPath).then(
      () => setCopied(true),
      () => {
        /* clipboard blocked; leave the button label unchanged */
      },
    );
  };

  const doClear = async () => {
    await clearVoiceMemory();
    setConfirmingClear(false);
    await load();
  };

  return (
    <div style={{ maxWidth: 760 }}>
      <PageTitle
        sub={
          <span style={{ display: "inline-flex", alignItems: "flex-start", gap: 7 }}>
            <Icon name="lock" size={14} style={{ color: theme.accentDeep, marginTop: 3 }} />
            <span>
              Everything below is learned locally, stored encrypted on this machine (AES-256, key in
              the OS keychain), and never uploaded.
            </span>
          </span>
        }
      >
        Voice Memory
      </PageTitle>

      {/* Learned corrections: the audit list itself. */}
      <Card style={{ marginBottom: 16 }}>
        <SectionTitle>
          <Icon name="book" size={15} style={{ color: theme.accentDeep }} />
          Learned corrections
          {corrections.length > 0 && (
            <span style={{ color: theme.textFaint, fontWeight: 500 }}>({corrections.length})</span>
          )}
        </SectionTitle>
        {corrections.length === 0 ? (
          <div style={{ padding: "26px 8px", textAlign: "center", color: theme.textFaint, fontSize: 13.5 }}>
            No corrections yet. When you fix a transcript WhimprFlow pasted, it notices the change
            and records the correction here.
          </div>
        ) : (
          <div>
            {corrections.map((ev, i) => (
              <CorrectionRow key={`${ev.ts_unix}-${i}`} ev={ev} />
            ))}
          </div>
        )}
      </Card>

      {/* Auto-learned vocabulary: the dictionary entries corrections produced. */}
      <Card style={{ marginBottom: 16 }}>
        <SectionTitle>
          <Icon name="sparkles" size={15} style={{ color: theme.accentDeep }} />
          Auto-learned vocabulary
          {autoWords.length > 0 && (
            <span style={{ color: theme.textFaint, fontWeight: 500 }}>({autoWords.length})</span>
          )}
        </SectionTitle>
        {autoWords.length === 0 ? (
          <p style={{ margin: 0, fontSize: 13.5, color: theme.textFaint }}>
            Nothing auto-learned yet. Words WhimprFlow picks up from your corrections will appear
            here and in the Dictionary pane.
          </p>
        ) : (
          <>
            <p style={{ margin: "0 0 12px", fontSize: 13, color: theme.textMuted, lineHeight: 1.5 }}>
              WhimprFlow picked up {autoWords.length === 1 ? "this word" : "these words"} on its own
              from your corrections. Review or remove any of them in the Dictionary pane.
            </p>
            <div style={{ display: "flex", flexWrap: "wrap", gap: 7 }}>
              {autoWords.map((w) => (
                <span
                  key={w}
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    padding: "4px 11px",
                    borderRadius: 999,
                    background: theme.accentSoft,
                    border: `1px solid ${theme.accentSoftBorder}`,
                    color: theme.accentDeep,
                    fontSize: 12.5,
                    fontWeight: 600,
                  }}
                >
                  {w}
                </span>
              ))}
            </div>
          </>
        )}
      </Card>

      {/* Export: plain-JSON bundle, path shown as a copyable code line. */}
      <Card style={{ marginBottom: 16 }}>
        <SectionTitle>
          <Icon name="archive" size={15} style={{ color: theme.accentDeep }} />
          Export
        </SectionTitle>
        <p style={{ margin: "0 0 14px", fontSize: 13, color: theme.textMuted, lineHeight: 1.5 }}>
          Writes everything WhimprFlow has learned (dictionary, snippets, style, and corrections) to
          a plain JSON file. The export is readable anywhere and not encrypted, so keep it somewhere
          you trust.
        </p>
        <Button variant="accent" onClick={() => void doExport()} disabled={exporting}>
          {exporting ? "Exporting..." : "Export as JSON"}
        </Button>
        {exportPath && (
          <div style={{ marginTop: 14 }}>
            <div style={{ fontSize: 12, color: theme.textMuted, marginBottom: 6 }}>Saved to</div>
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <code
                style={{
                  flex: 1,
                  minWidth: 0,
                  display: "block",
                  fontFamily: font.mono,
                  fontSize: 12,
                  color: theme.textBody,
                  background: theme.cardBgSubtle,
                  border: `1px solid ${theme.border}`,
                  borderRadius: 8,
                  padding: "8px 10px",
                  overflowX: "auto",
                  whiteSpace: "nowrap",
                }}
              >
                {exportPath}
              </code>
              <Button variant="ghost" size="sm" onClick={copyPath}>
                {copied ? "Copied" : "Copy"}
              </Button>
            </div>
          </div>
        )}
        {exportError && (
          <div style={{ marginTop: 12, fontSize: 12.5, color: palette.error }}>
            Export failed: {exportError}
          </div>
        )}
      </Card>

      {/* Clear: destructive, so it takes two clicks. */}
      <Card>
        <SectionTitle>
          <Icon name="shield" size={15} style={{ color: theme.accentDeep }} />
          Clear voice memory
        </SectionTitle>
        <p style={{ margin: "0 0 14px", fontSize: 13, color: theme.textMuted, lineHeight: 1.5 }}>
          Erases the encrypted corrections log on this machine. Dictionary entries, snippets, and
          your style profile are kept; manage those in their own panes.
        </p>
        {confirmingClear ? (
          <div style={{ display: "flex", alignItems: "center", flexWrap: "wrap", gap: 10 }}>
            <span style={{ fontSize: 13, fontWeight: 600, color: palette.error }}>
              Erase all learned corrections? This cannot be undone.
            </span>
            <Button variant="dark" onClick={() => void doClear()}>
              Yes, erase
            </Button>
            <Button variant="ghost" onClick={() => setConfirmingClear(false)}>
              Cancel
            </Button>
          </div>
        ) : (
          <Button variant="ghost" onClick={() => setConfirmingClear(true)}>
            Clear voice memory
          </Button>
        )}
      </Card>
    </div>
  );
}
