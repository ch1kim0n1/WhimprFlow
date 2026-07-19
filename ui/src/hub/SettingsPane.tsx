import { useState } from "react";
import { font } from "../tokens/values";
import { theme, applyTheme, getStoredTheme, type ThemeMode } from "./theme";
import { Button, Card, Dot, PageTitle, Segmented } from "./ui";
import { Icon, type IconName } from "./icons";
import {
  requestAccessibility,
  requestInputMonitoring,
  requestMicrophone,
  setApiKey,
  type CleanupLevel,
  type CleanupMode,
  type Settings,
  type Status,
} from "./api";

const MODES: { value: CleanupMode; label: string; hint: string }[] = [
  { value: "raw", label: "Raw", hint: "Paste exactly what you said" },
  { value: "local", label: "Local", hint: "On-device model (offline)" },
  { value: "open_ai", label: "OpenAI", hint: "Cloud cleanup via OpenAI (or an OpenAI-compatible API like OpenRouter - set the base URL below)" },
  { value: "anthropic", label: "Anthropic", hint: "Cloud cleanup via Claude" },
];

const LEVELS: { value: CleanupLevel; label: string; hint: string }[] = [
  { value: "none", label: "None", hint: "Transcribe exactly what you said, including mistakes." },
  { value: "light", label: "Light", hint: "Clean up filler words and grammar. (Recommended)" },
  { value: "medium", label: "Medium", hint: "Edit for clarity and conciseness." },
  { value: "high", label: "High", hint: "Rewrite for brevity and polish." },
];

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
      {sub && <div style={{ color: theme.textMuted, fontSize: 13, marginTop: 4 }}>{sub}</div>}
    </div>
  );
}

function KeyField({
  label,
  configured,
  onSave,
}: {
  label: string;
  configured: boolean;
  onSave: (key: string) => void;
}) {
  const [value, setValue] = useState("");
  const [saved, setSaved] = useState(false);
  return (
    <div style={{ marginTop: 16 }}>
      <div style={{ fontSize: 13, marginBottom: 7, display: "flex", alignItems: "center", color: theme.textBody }}>
        <Dot ok={configured} />
        {label} {configured ? " - configured" : " - not set"}
      </div>
      <div style={{ display: "flex", gap: 8 }}>
        <input
          type="password"
          value={value}
          placeholder={configured ? "Enter a new key to replace" : "Paste your API key"}
          onChange={(e) => {
            setValue(e.target.value);
            setSaved(false);
          }}
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
        <Button
          onClick={() => {
            onSave(value);
            setValue("");
            setSaved(true);
          }}
        >
          Save
        </Button>
      </div>
      {saved && <div style={{ fontSize: 12, color: theme.accentDeep, marginTop: 6 }}>Saved to keychain ✓</div>}
    </div>
  );
}

function PermRow({
  ok,
  label,
  detail,
  icon,
  onClick,
}: {
  ok: boolean;
  label: string;
  detail: string;
  icon: IconName;
  onClick: () => void;
}) {
  return (
    <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
      <div style={{ display: "flex", alignItems: "center", fontSize: 13 }}>
        <span
          style={{
            width: 24,
            height: 24,
            borderRadius: 8,
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
            background: ok ? theme.accentSoft : theme.cardBgSubtle,
            color: ok ? theme.accentDeep : theme.textMuted,
            marginRight: 8,
            flex: "0 0 auto",
          }}
        >
          <Icon name={ok ? "check" : icon} size={13} strokeWidth={1.8} />
        </span>
        <span style={{ color: theme.textBody }}>
          <b>{label}</b> <span style={{ color: theme.textMuted }}> - {detail}</span>
        </span>
      </div>
      {ok ? (
        <span style={{ color: theme.accentDeep, fontSize: 13, fontWeight: 600 }}>Granted</span>
      ) : (
        <Button variant="ghost" size="sm" onClick={onClick}>
          Grant
        </Button>
      )}
    </div>
  );
}

function GitHubMark() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
      <path d="M12 2C6.48 2 2 6.59 2 12.25c0 4.53 2.87 8.37 6.84 9.73.5.1.68-.22.68-.49 0-.24-.01-1.05-.01-1.91-2.78.62-3.37-1.2-3.37-1.2-.45-1.18-1.11-1.49-1.11-1.49-.91-.64.07-.63.07-.63 1 .08 1.54 1.06 1.54 1.06.9 1.57 2.35 1.12 2.92.86.09-.67.35-1.12.64-1.38-2.22-.26-4.56-1.15-4.56-5.1 0-1.13.39-2.05 1.03-2.77-.1-.26-.45-1.31.1-2.73 0 0 .84-.28 2.75 1.06A9.28 9.28 0 0 1 12 7.16c.85 0 1.71.12 2.51.34 1.91-1.34 2.75-1.06 2.75-1.06.55 1.42.2 2.47.1 2.73.64.72 1.03 1.64 1.03 2.77 0 3.96-2.35 4.83-4.58 5.09.36.33.68.96.68 1.94 0 1.4-.01 2.53-.01 2.87 0 .27.18.59.69.49A10.26 10.26 0 0 0 22 12.25C22 6.59 17.52 2 12 2Z" />
    </svg>
  );
}

export function SettingsPane({
  settings,
  onChange,
  status,
  refresh,
}: {
  settings: Settings;
  onChange: (s: Settings) => void;
  status: Status;
  refresh: () => void;
}) {
  const [appearance, setAppearance] = useState<ThemeMode>(getStoredTheme());
  return (
    <div style={{ maxWidth: 720 }}>
      <PageTitle>Settings</PageTitle>

      <Card style={{ marginBottom: 16 }}>
        <SectionTitle sub="Switch between the warm light theme and a low-glare dark theme.">
          Appearance
        </SectionTitle>
        <Segmented
          options={[
            { value: "light", label: "Light" },
            { value: "dark", label: "Dark" },
          ]}
          value={appearance}
          onChange={(v) => {
            setAppearance(v);
            applyTheme(v);
          }}
        />
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <SectionTitle icon="cloud" sub="Where your dictation is cleaned up before it's typed.">
          Cleanup Engine
        </SectionTitle>
        <Segmented
          options={MODES.map((m) => ({ value: m.value, label: m.label }))}
          value={settings.cleanup_mode}
          onChange={(v) => onChange({ ...settings, cleanup_mode: v })}
        />
        <div style={{ color: theme.textMuted, fontSize: 12.5, marginTop: 10 }}>
          {MODES.find((m) => m.value === settings.cleanup_mode)?.hint}
        </div>

        <KeyField
          label="OpenAI API key"
          configured={status.has_openai_key}
          onSave={(k) => {
            setApiKey("openai", k);
            setTimeout(refresh, 400);
          }}
        />
        <div style={{ marginTop: 12, display: "flex", gap: 8 }}>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 12.5, color: theme.textMuted, marginBottom: 6 }}>
              Base URL (blank = OpenAI; e.g. https://openrouter.ai/api/v1 for OpenRouter)
            </div>
            <input
              type="text"
              value={settings.openai_base_url}
              placeholder="https://openrouter.ai/api/v1"
              onChange={(e) => onChange({ ...settings, openai_base_url: e.target.value })}
              style={{
                width: "100%",
                background: theme.cardBgSubtle,
                border: `1px solid ${theme.border}`,
                borderRadius: 10,
                padding: "9px 12px",
                color: theme.textBody,
                fontFamily: font.mono,
                fontSize: 13,
                outline: "none",
                boxSizing: "border-box",
              }}
            />
          </div>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 12.5, color: theme.textMuted, marginBottom: 6 }}>
              Model (e.g. an OpenRouter model slug)
            </div>
            <input
              type="text"
              value={settings.openai_model}
              placeholder="meta-llama/llama-3.3-70b-instruct:free"
              onChange={(e) => onChange({ ...settings, openai_model: e.target.value })}
              style={{
                width: "100%",
                background: theme.cardBgSubtle,
                border: `1px solid ${theme.border}`,
                borderRadius: 10,
                padding: "9px 12px",
                color: theme.textBody,
                fontFamily: font.mono,
                fontSize: 13,
                outline: "none",
                boxSizing: "border-box",
              }}
            />
          </div>
        </div>
        <KeyField
          label="Anthropic API key"
          configured={status.has_anthropic_key}
          onSave={(k) => {
            setApiKey("anthropic", k);
            setTimeout(refresh, 400);
          }}
        />
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <SectionTitle icon="sparkles">Auto Cleanup</SectionTitle>
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          {LEVELS.map((l) => {
            const selected = settings.cleanup_level === l.value;
            return (
              <button
                key={l.value}
                onClick={() => onChange({ ...settings, cleanup_level: l.value })}
                style={{
                  textAlign: "left",
                  cursor: "pointer",
                  borderRadius: 12,
                  padding: "12px 14px",
                  fontFamily: font.ui,
                  background: selected ? theme.accentSoft : theme.cardBgSubtle,
                  border: `1px solid ${selected ? theme.accentSoftBorder : theme.border}`,
                  color: theme.textBody,
                }}
              >
                <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>{l.label}</div>
                <div style={{ fontSize: 12.5, color: theme.textMuted, marginTop: 2 }}>{l.hint}</div>
              </button>
            );
          })}
        </div>
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 14, fontWeight: 600, color: theme.textStrong }}>
            <Icon name="keyboard" size={16} strokeWidth={1.8} />
            Play a sound when recording starts
          </div>
          <Segmented
            options={[
              { value: "on", label: "On" },
              { value: "off", label: "Off" },
            ]}
            value={settings.sound_on_start ? "on" : "off"}
            onChange={(v) => onChange({ ...settings, sound_on_start: v === "on" })}
          />
        </div>
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <SectionTitle icon="shield" sub="Replace inappropriate words and curses in the text WhimprFlow inserts. This is off by default.">
          Safe Mode
        </SectionTitle>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 16 }}>
          <div style={{ color: theme.textMuted, fontSize: 13, lineHeight: 1.45, maxWidth: 430 }}>
            Redaction happens after cleanup, so it also protects transcripts returned by cloud providers.
          </div>
          <Segmented
            options={[
              { value: "on", label: "On" },
              { value: "off", label: "Off" },
            ]}
            value={settings.safe_mode ? "on" : "off"}
            onChange={(value) => onChange({ ...settings, safe_mode: value === "on" })}
          />
        </div>
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <SectionTitle icon="shield" sub="Grant these to WhimprFlow, then quit and reopen the app if a dot stays grey.">
          Permissions
        </SectionTitle>
        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          <PermRow
            icon="lock"
            ok={status.accessibility}
            label="Accessibility"
            detail={
              status.accessibility
                ? "granted - Fn works everywhere + types your words"
                : "the key one: makes Fn work in every app and types your words"
            }
            onClick={() => {
              requestAccessibility();
              setTimeout(refresh, 800);
            }}
          />
          <PermRow
            icon="mic"
            ok={status.microphone}
            label="Microphone"
            detail={status.microphone ? "granted" : "hears what you say"}
            onClick={() => {
              requestMicrophone();
              setTimeout(refresh, 1000);
            }}
          />
          <PermRow
            icon="shield"
            ok={status.input_monitoring}
            label="Input Monitoring"
            detail="optional - extra reliability for key detection"
            onClick={() => {
              requestInputMonitoring();
              setTimeout(refresh, 1000);
            }}
          />
        </div>
      </Card>

      <Card>
        <SectionTitle sub="WhimprFlow is built in the open.">About</SectionTitle>
        <a
          href="https://github.com/Blueturboguy07/WhimprFlow"
          target="_blank"
          rel="noreferrer"
          style={{ display: "inline-flex", alignItems: "center", gap: 9, borderRadius: 10, padding: "10px 13px", color: theme.textStrong, background: theme.cardBgSubtle, border: `1px solid ${theme.border}`, textDecoration: "none", fontSize: 13.5, fontWeight: 650 }}
        >
          <GitHubMark />
          View on GitHub
        </a>
      </Card>
    </div>
  );
}
