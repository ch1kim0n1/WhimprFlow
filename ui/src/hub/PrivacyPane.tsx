import { useEffect, useState } from "react";
import { font, palette } from "../tokens/values";
import { theme } from "./theme";
import { Button, Card, PageTitle, Segmented } from "./ui";
import { Icon, type IconName } from "./icons";
import { dayLabel, fmtTimeOfDay } from "./format";
import {
  clearHistoryText,
  getLastCapsule,
  getLedger,
  type CapsuleReport,
  type HistoryItem,
  type Settings,
  type Status,
} from "./api";

// ── Retention choices ─────────────────────────────────────────────────────────
// Bound to settings.retention_days: null = forever, 0 = never store text.
type RetentionKey = "forever" | "30" | "7" | "1" | "0";

const RETENTION: { value: RetentionKey; label: string; hint: string }[] = [
  { value: "forever", label: "Forever", hint: "Transcript text stays until you delete it." },
  { value: "30", label: "30 days", hint: "Transcript text older than 30 days is deleted automatically." },
  { value: "7", label: "7 days", hint: "Transcript text older than 7 days is deleted automatically." },
  { value: "1", label: "1 day", hint: "Transcript text older than a day is deleted automatically." },
  {
    value: "0",
    label: "Never store",
    hint: "Never store transcript text. New dictations record only the numbers; the text is dropped immediately.",
  },
];

// ── Shared bits (mirrors SettingsPane's private helpers) ─────────────────────
function SectionTitle({
  children,
  sub,
  icon,
}: {
  children: React.ReactNode;
  sub?: string;
  icon?: IconName;
}) {
  return (
    <div style={{ marginBottom: 14 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 15, fontWeight: 600, color: theme.textStrong }}>
        {icon && (
          <span
            style={{
              width: 24,
              height: 24,
              borderRadius: 8,
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              background: theme.accentSoft,
              color: theme.accentDeep,
              flex: "0 0 auto",
            }}
          >
            <Icon name={icon} size={13} strokeWidth={1.8} />
          </span>
        )}
        <span>{children}</span>
      </div>
      {sub && <div style={{ color: theme.textMuted, fontSize: 13, marginTop: 4, lineHeight: 1.45 }}>{sub}</div>}
    </div>
  );
}

function ToggleRow({
  label,
  detail,
  value,
  onChange,
}: {
  label: string;
  detail?: string;
  value: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 16 }}>
      <div>
        <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>{label}</div>
        {detail && (
          <div style={{ fontSize: 12.5, color: theme.textMuted, marginTop: 2, lineHeight: 1.45, maxWidth: 430 }}>
            {detail}
          </div>
        )}
      </div>
      <Segmented
        options={[
          { value: "on", label: "On" },
          { value: "off", label: "Off" },
        ]}
        value={value ? "on" : "off"}
        onChange={(v) => onChange(v === "on")}
      />
    </div>
  );
}

// ── Delete-all-text (two-step confirm) ───────────────────────────────────────
function DeleteAllText() {
  const [confirming, setConfirming] = useState(false);
  const [scrubbed, setScrubbed] = useState<number | null>(null);

  const run = async () => {
    const n = await clearHistoryText();
    setScrubbed(n);
    setConfirming(false);
  };

  return (
    <div>
      {confirming ? (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 12,
            background: "rgba(255,107,107,0.08)",
            border: `1px solid rgba(255,107,107,0.35)`,
            borderRadius: 12,
            padding: "12px 14px",
          }}
        >
          <div style={{ fontSize: 13, color: theme.textBody, lineHeight: 1.45 }}>
            This permanently removes the text of every stored dictation. Word counts and speed stats stay.
          </div>
          <div style={{ display: "flex", gap: 8, flex: "0 0 auto" }}>
            <Button variant="ghost" size="sm" onClick={() => setConfirming(false)}>
              Cancel
            </Button>
            <Button size="sm" onClick={() => void run()}>
              Yes, delete text
            </Button>
          </div>
        </div>
      ) : (
        <Button variant="ghost" onClick={() => setConfirming(true)}>
          Delete all stored transcript text
        </Button>
      )}
      {scrubbed !== null && !confirming && (
        <div style={{ fontSize: 12.5, color: theme.accentDeep, marginTop: 8 }}>
          Scrubbed text from {scrubbed} {scrubbed === 1 ? "record" : "records"}. Stats were kept.
        </div>
      )}
    </div>
  );
}

// ── Capsule allowlist editor ─────────────────────────────────────────────────
function AppAllowlist({ apps, onChange }: { apps: string[]; onChange: (apps: string[]) => void }) {
  const [draft, setDraft] = useState("");

  const add = () => {
    const id = draft.trim();
    if (!id || apps.includes(id)) return;
    onChange([...apps, id]);
    setDraft("");
  };

  return (
    <div>
      <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>Limit to these apps</div>
      <div style={{ fontSize: 12.5, color: theme.textMuted, marginTop: 2, lineHeight: 1.45, maxWidth: 430 }}>
        Bundle ids where the capsule may be captured. Leave the list empty to allow every app while the
        capsule is on.
      </div>
      <div style={{ display: "flex", gap: 8, marginTop: 10 }}>
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") add();
          }}
          placeholder="com.apple.Safari"
          aria-label="Bundle id to allow"
          style={{
            flex: 1,
            background: theme.cardBgSubtle,
            border: `1px solid ${theme.border}`,
            borderRadius: 10,
            padding: "9px 12px",
            color: theme.textBody,
            fontFamily: font.mono,
            fontSize: 13,
            outline: "none",
          }}
        />
        <Button variant="ghost" size="sm" onClick={add} disabled={!draft.trim()}>
          <Icon name="plus" size={14} />
          Add
        </Button>
      </div>
      {apps.length > 0 && (
        <div style={{ display: "flex", flexWrap: "wrap", gap: 8, marginTop: 10 }}>
          {apps.map((id) => (
            <span
              key={id}
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 6,
                background: theme.cardBgSubtle,
                border: `1px solid ${theme.border}`,
                borderRadius: 999,
                padding: "4px 6px 4px 11px",
                fontFamily: font.mono,
                fontSize: 12,
                color: theme.textBody,
              }}
            >
              {id}
              <button
                onClick={() => onChange(apps.filter((a) => a !== id))}
                aria-label={`Remove ${id}`}
                title={`Remove ${id}`}
                style={{
                  border: "none",
                  background: "transparent",
                  cursor: "pointer",
                  color: theme.textFaint,
                  display: "flex",
                  alignItems: "center",
                  padding: 2,
                }}
              >
                <Icon name="close" size={13} />
              </button>
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

// ── "What would be shared" viewer ────────────────────────────────────────────
function CapsuleField({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div style={{ display: "flex", gap: 10, fontSize: 13, lineHeight: 1.5 }}>
      <div style={{ flex: "0 0 92px", color: theme.textMuted }}>{label}</div>
      <div style={{ color: theme.textBody, minWidth: 0, overflowWrap: "anywhere" }}>{children}</div>
    </div>
  );
}

function CapsuleViewer({ capsule, loaded }: { capsule: CapsuleReport | null; loaded: boolean }) {
  if (!loaded) {
    return <div style={{ fontSize: 13, color: theme.textFaint }}>Loading the last capsule...</div>;
  }
  if (!capsule) {
    return (
      <div style={{ fontSize: 13, color: theme.textFaint, lineHeight: 1.5 }}>
        No capsule captured yet this run. Dictate once and the exact bundle a cleanup request would
        include appears here.
      </div>
    );
  }
  const glossary =
    capsule.glossary.length === 0
      ? "none"
      : capsule.glossary.length <= 8
        ? capsule.glossary.join(", ")
        : `${capsule.glossary.slice(0, 8).join(", ")} +${capsule.glossary.length - 8} more`;
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      {!capsule.enabled && (
        <div style={{ fontSize: 12.5, color: theme.textMuted }}>
          Captured while the capsule was off, so nothing below was shared.
        </div>
      )}
      <CapsuleField label="App">{capsule.app ?? "none"}</CapsuleField>
      <CapsuleField label="Selection">
        {capsule.selection_preview ? (
          <span style={{ fontFamily: font.mono, fontSize: 12.5 }}>"{capsule.selection_preview}"</span>
        ) : (
          "not included"
        )}
      </CapsuleField>
      <CapsuleField label="Glossary">{glossary}</CapsuleField>
      <CapsuleField label="Style">{capsule.style ? "included" : "not included"}</CapsuleField>
    </div>
  );
}

// ── Per-dictation ledger ─────────────────────────────────────────────────────
// ponytail: the ledger renders only the newest 200 dictations in a flat table;
// upgrade path is a virtualized list (or paging) once history outgrows that.
const MAX_LEDGER_ROWS = 200;

function GateCell({ gate }: { gate: string }) {
  const color =
    gate === "passed" ? theme.accentDeep : gate === "rejected" ? palette.error : theme.textFaint;
  return <span style={{ color, fontWeight: 600 }}>{gate || "-"}</span>;
}

function RouteBadge({ cloud }: { cloud: boolean }) {
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: 6, fontWeight: 600 }}>
      <span
        style={{
          width: 7,
          height: 7,
          borderRadius: 999,
          background: cloud ? palette.warn : palette.success,
          flex: "0 0 auto",
        }}
      />
      {cloud ? "Cloud" : "Local"}
    </span>
  );
}

function Ledger({ items }: { items: HistoryItem[] }) {
  const th = {
    textAlign: "left" as const,
    fontSize: 11,
    fontWeight: 600,
    textTransform: "uppercase" as const,
    letterSpacing: 0.6,
    color: theme.textFaint,
    padding: "8px 10px",
    borderBottom: `1px solid ${theme.border}`,
    whiteSpace: "nowrap" as const,
  };
  const td = {
    fontSize: 13,
    color: theme.textBody,
    padding: "9px 10px",
    borderBottom: `1px solid ${theme.border}`,
    whiteSpace: "nowrap" as const,
    verticalAlign: "top" as const,
  };

  if (items.length === 0) {
    return (
      <div style={{ padding: "26px 8px", textAlign: "center", color: theme.textFaint, fontSize: 13.5 }}>
        No dictations recorded yet. Each one you make gets a row here.
      </div>
    );
  }

  return (
    <div style={{ overflowX: "auto" }}>
      <table style={{ borderCollapse: "collapse", width: "100%", fontFamily: font.ui }}>
        <thead>
          <tr>
            <th style={th}>When</th>
            <th style={th}>App</th>
            <th style={th}>Engine</th>
            <th style={th}>Cleanup</th>
            <th style={th}>Route</th>
            <th style={th}>Gate</th>
          </tr>
        </thead>
        <tbody>
          {items.map((it, i) => {
            const d = new Date(it.ts_unix * 1000);
            const p = it.provenance;
            const cloud = p.sent_to_cloud;
            // Show just the model file; the full engine string sits in the tooltip.
            const engine = p.asr_engine.includes(":") ? p.asr_engine.split(":").pop() : p.asr_engine;
            return (
              <tr key={`${it.ts_unix}-${i}`} style={{ background: cloud ? "rgba(245,180,84,0.08)" : "transparent" }}>
                <td style={{ ...td, color: theme.textMuted, fontVariantNumeric: "tabular-nums" }}>
                  {dayLabel(d)} {fmtTimeOfDay(d)}
                </td>
                <td style={td}>{it.app ?? "-"}</td>
                <td style={{ ...td, fontFamily: font.mono, fontSize: 12 }} title={p.asr_engine}>
                  {engine || "-"}
                </td>
                <td style={{ ...td, fontFamily: font.mono, fontSize: 12 }}>{p.cleanup || "-"}</td>
                <td style={td}>
                  <RouteBadge cloud={cloud} />
                </td>
                <td style={td}>
                  <GateCell gate={p.gate} />
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

// ── Page ──────────────────────────────────────────────────────────────────────
export function PrivacyPane({
  settings,
  onChange,
}: {
  settings: Settings;
  onChange: (s: Settings) => void;
  status?: Status;
}) {
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [capsule, setCapsule] = useState<CapsuleReport | null>(null);
  const [capsuleLoaded, setCapsuleLoaded] = useState(false);

  const loadCapsule = () =>
    getLastCapsule().then((c) => {
      setCapsule(c);
      setCapsuleLoaded(true);
    });

  useEffect(() => {
    void loadCapsule();
    // getLedger, not getHistory: the ledger audits every dictation, and
    // getHistory drops textless records - exactly what "Never store" produces.
    void getLedger(MAX_LEDGER_ROWS).then((h) =>
      setHistory([...h].sort((a, b) => b.ts_unix - a.ts_unix).slice(0, MAX_LEDGER_ROWS)),
    );
  }, []);

  const retentionValue = (
    settings.retention_days === null ? "forever" : String(settings.retention_days)
  ) as RetentionKey;
  const retentionHint =
    RETENTION.find((r) => r.value === retentionValue)?.hint ??
    `Custom: text older than ${settings.retention_days} days is deleted.`;

  return (
    <div style={{ maxWidth: 760 }}>
      <PageTitle
        sub={
          "Local by default: your voice is transcribed on this device and audio never leaves it. " +
          "Transcript text goes out only when you pick a cloud cleanup engine, and the ledger below " +
          "records every time that happens."
        }
      >
        Privacy
      </PageTitle>

      <Card style={{ marginBottom: 16 }}>
        <SectionTitle
          icon="archive"
          sub="How long the text of your dictations is kept in history. Word counts, speed, and streaks are always kept; retention only deletes the text."
        >
          Transcript retention
        </SectionTitle>
        <Segmented
          options={RETENTION.map((r) => ({ value: r.value, label: r.label }))}
          value={retentionValue}
          onChange={(v) => onChange({ ...settings, retention_days: v === "forever" ? null : Number(v) })}
        />
        <div style={{ color: theme.textMuted, fontSize: 12.5, marginTop: 10 }}>{retentionHint}</div>

        <div style={{ borderTop: `1px solid ${theme.border}`, marginTop: 18, paddingTop: 16 }}>
          <DeleteAllText />
        </div>
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <SectionTitle
          icon="lock"
          sub="Off by default. When on, a cleanup request can include a small bundle of context - the app you're dictating into, your dictionary glossary, and your writing style - so the result fits where it lands."
        >
          Context Capsule
        </SectionTitle>
        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          <ToggleRow
            label="Share app context with cleanup"
            detail="Captured at record start. With a cloud engine this context is sent along with your words; with local cleanup it never leaves the device."
            value={settings.capsule.enabled}
            onChange={(v) => onChange({ ...settings, capsule: { ...settings.capsule, enabled: v } })}
          />
          <ToggleRow
            label="Include selected text"
            detail="Also include the text currently selected in the target app. This is the most revealing part, so it is a separate switch."
            value={settings.capsule.include_selection}
            onChange={(v) =>
              onChange({ ...settings, capsule: { ...settings.capsule, include_selection: v } })
            }
          />
          <AppAllowlist
            apps={settings.capsule.apps}
            onChange={(apps) => onChange({ ...settings, capsule: { ...settings.capsule, apps } })}
          />
        </div>

        <div style={{ borderTop: `1px solid ${theme.border}`, marginTop: 18, paddingTop: 16 }}>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12, marginBottom: 12 }}>
            <div>
              <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>What would be shared</div>
              <div style={{ fontSize: 12.5, color: theme.textMuted, marginTop: 2 }}>
                The exact capsule from your most recent dictation.
              </div>
            </div>
            <Button variant="ghost" size="sm" onClick={() => void loadCapsule()}>
              Refresh
            </Button>
          </div>
          <CapsuleViewer capsule={capsule} loaded={capsuleLoaded} />
        </div>
      </Card>

      <Card pad={0}>
        <div style={{ padding: "22px 22px 0" }}>
          <SectionTitle
            icon="shield"
            sub="One row per dictation: which engine heard it, how it was cleaned up, and whether it left the device. Amber rows went to a cloud engine."
          >
            Dictation ledger
          </SectionTitle>
        </div>
        <div style={{ padding: "0 10px 10px" }}>
          <Ledger items={history} />
        </div>
      </Card>
    </div>
  );
}
