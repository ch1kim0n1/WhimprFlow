import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { font } from "../tokens/values";
import { theme, applyTheme, getStoredTheme, type ThemeMode } from "./theme";
import { Button, Card, Dot, PageTitle, Segmented } from "./ui";
import { Icon, type IconName } from "./icons";
import {
  backupData,
  checkForUpdate,
  exportDiagnostics,
  listCrashReports,
  listBackups,
  relaunchAfterUpdate,
  requestAccessibility,
  requestInputMonitoring,
  requestMicrophone,
  restoreBackup,
  setApiKey,
  type AvailableUpdate,
  type CleanupLevel,
  type CleanupMode,
  type Settings,
  type Status,
} from "./api";

const APP_VERSION = "1.0.0";

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

// Common dictation languages, as whisper.cpp language codes. "Auto" (stored as
// language: null) lets the model detect the language per session.
const LANGUAGES: { value: string; label: string }[] = [
  { value: "en", label: "English" },
  { value: "es", label: "Spanish" },
  { value: "fr", label: "French" },
  { value: "de", label: "German" },
  { value: "it", label: "Italian" },
  { value: "pt", label: "Portuguese" },
  { value: "nl", label: "Dutch" },
  { value: "pl", label: "Polish" },
  { value: "ru", label: "Russian" },
  { value: "uk", label: "Ukrainian" },
  { value: "ja", label: "Japanese" },
  { value: "ko", label: "Korean" },
  { value: "zh", label: "Chinese" },
  { value: "hi", label: "Hindi" },
  { value: "ar", label: "Arabic" },
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

// A label (+ optional detail) with an On/Off control, matching the pane's
// existing row style (see Safe Mode / record-start sound).
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

type UpdateState = "idle" | "checking" | "none" | "available" | "downloading" | "ready" | "error";

export function UpdatesCard() {
  const [state, setState] = useState<UpdateState>("idle");
  const [update, setUpdate] = useState<AvailableUpdate | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  async function check() {
    setState("checking");
    setMessage(null);
    const found = await checkForUpdate();
    if (!found) {
      setState("none");
      setMessage("You're on the latest version.");
      return;
    }
    setUpdate(found);
    setState("available");
  }

  async function downloadAndInstall() {
    if (!update) return;
    setState("downloading");
    try {
      await update.downloadAndInstall();
      setState("ready");
      setMessage("Update installed. Relaunching...");
      await relaunchAfterUpdate();
    } catch {
      setState("error");
      setMessage("Update failed. Try again or download it from GitHub.");
    }
  }

  return (
    <Card style={{ marginBottom: 16 }}>
      <SectionTitle icon="sparkles" sub="Check for and install the latest WhimprFlow release.">
        Updates
      </SectionTitle>
      <div style={{ display: "flex", alignItems: "center", gap: 12, flexWrap: "wrap" }}>
        {state === "available" && update ? (
          <Button variant="accent" onClick={downloadAndInstall}>
            Install version {update.version}
          </Button>
        ) : (
          <Button onClick={check} disabled={state === "checking" || state === "downloading"}>
            {state === "checking"
              ? "Checking..."
              : state === "downloading"
                ? "Downloading..."
                : "Check for updates"}
          </Button>
        )}
        {message && (
          <span style={{ fontSize: 13, color: state === "error" ? theme.textMuted : theme.accentDeep }}>
            {message}
          </span>
        )}
        {state === "available" && update?.body && (
          <details style={{ width: "100%", marginTop: 4 }}>
            <summary style={{ cursor: "pointer", fontSize: 12.5, color: theme.textMuted }}>
              Release notes
            </summary>
            <pre
              style={{
                whiteSpace: "pre-wrap",
                fontFamily: font.ui,
                fontSize: 12.5,
                color: theme.textBody,
                marginTop: 8,
                background: theme.cardBgSubtle,
                border: `1px solid ${theme.border}`,
                borderRadius: 10,
                padding: 12,
              }}
            >
              {update.body}
            </pre>
          </details>
        )}
      </div>
    </Card>
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
  const { t, i18n } = useTranslation();
  const [appearance, setAppearance] = useState<ThemeMode>(getStoredTheme());
  return (
    <div style={{ maxWidth: 720 }}>
      <PageTitle>{t("settings.title")}</PageTitle>

      <Card style={{ marginBottom: 16 }}>
        <SectionTitle sub="Interface language. More locales can be contributed later.">
          {t("settings.language")}
        </SectionTitle>
        <Segmented
          options={[{ value: "en", label: t("settings.languageEnglish") }]}
          value={(i18n.language ?? "en").startsWith("en") ? "en" : "en"}
          onChange={(v) => {
            void i18n.changeLanguage(v);
          }}
        />
      </Card>

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
        {(settings.cleanup_mode === "open_ai" || settings.cleanup_mode === "anthropic") && (
          <div style={{ color: theme.textMuted, fontSize: 12.5, marginTop: 8, lineHeight: 1.45 }}>
            Cloud cleanup sends your raw transcript (and optional short app context) to the provider
            you configure. Requires an active license or trial (Hub &gt; License). See the{" "}
            <a
              href="https://github.com/ch1kim0n1/WhimprFlow/blob/main/docs/legal/PRIVACY.md"
              target="_blank"
              rel="noreferrer"
              style={{ color: theme.accentDeep }}
            >
              Privacy Policy
            </a>
            .
          </div>
        )}

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
        <SectionTitle icon="mic" sub="The language you dictate in. Auto detects it per session; picking one improves accuracy.">
          Language
        </SectionTitle>
        <select
          value={settings.language ?? "auto"}
          onChange={(e) =>
            onChange({ ...settings, language: e.target.value === "auto" ? null : e.target.value })
          }
          aria-label="Dictation language"
          style={{
            background: theme.cardBgSubtle,
            border: `1px solid ${theme.border}`,
            borderRadius: 10,
            padding: "9px 12px",
            color: theme.textBody,
            fontFamily: font.ui,
            fontSize: 13,
            outline: "none",
            minWidth: 220,
          }}
        >
          <option value="auto">Auto-detect</option>
          {LANGUAGES.map((l) => (
            <option key={l.value} value={l.value}>
              {l.label}
            </option>
          ))}
        </select>
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <SectionTitle icon="mic" sub="How dictation behaves while you speak and where it lands.">
          Dictation
        </SectionTitle>
        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          <ToggleRow
            label="Live preview while speaking"
            detail="Show provisional text in the floating pill as you talk. Nothing is typed until you finish."
            value={settings.streaming_preview}
            onChange={(v) => onChange({ ...settings, streaming_preview: v })}
          />
          <ToggleRow
            label="Meeting mode"
            detail="Hands-free sessions go to Studio notes, not the cursor."
            value={settings.meeting_mode}
            onChange={(v) => onChange({ ...settings, meeting_mode: v })}
          />
          <ToggleRow
            label="Code Mode in IDEs and terminals"
            detail="Keep identifiers, casing, and symbols verbatim when dictating into a code editor or terminal."
            value={settings.code_mode_auto}
            onChange={(v) => onChange({ ...settings, code_mode_auto: v })}
          />
        </div>
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <SectionTitle icon="keyboard" sub="Optional cues when a dictation starts or finishes.">
          Audio feedback
        </SectionTitle>
        <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
            <div style={{ fontSize: 13.5, color: theme.textMuted }}>Sound when recording starts</div>
            <Segmented
              options={[
                { value: "on", label: "On" },
                { value: "off", label: "Off" },
              ]}
              value={settings.sound_on_start ? "on" : "off"}
              onChange={(v) => onChange({ ...settings, sound_on_start: v === "on" })}
            />
          </div>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
            <div style={{ fontSize: 13.5, color: theme.textMuted }}>Sound when dictation completes</div>
            <Segmented
              options={[
                { value: "on", label: "On" },
                { value: "off", label: "Off" },
              ]}
              value={settings.sound_on_complete ? "on" : "off"}
              onChange={(v) => onChange({ ...settings, sound_on_complete: v === "on" })}
            />
          </div>
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

      <UpdatesCard />

      <Card style={{ marginBottom: 16 }}>
        <SectionTitle icon="sparkles" sub="Trade RAM for responsiveness. 0 means unlimited / never.">
          Performance
        </SectionTitle>
        <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          <label style={{ fontSize: 13, color: theme.textMuted }}>
            Max local LLM RAM hint (MB)
            <input
              type="number"
              min={0}
              value={settings.max_ram_mb}
              onChange={(e) =>
                onChange({ ...settings, max_ram_mb: Math.max(0, Number(e.target.value) || 0) })
              }
              style={{
                display: "block",
                marginTop: 6,
                width: 160,
                padding: "8px 10px",
                borderRadius: 10,
                border: `1px solid ${theme.border}`,
                background: theme.cardBgSubtle,
                color: theme.textStrong,
              }}
            />
          </label>
          <label style={{ fontSize: 13, color: theme.textMuted }}>
            Unload Whisper after idle (minutes)
            <input
              type="number"
              min={0}
              value={settings.unload_asr_after_idle_minutes}
              onChange={(e) =>
                onChange({
                  ...settings,
                  unload_asr_after_idle_minutes: Math.max(0, Number(e.target.value) || 0),
                })
              }
              style={{
                display: "block",
                marginTop: 6,
                width: 160,
                padding: "8px 10px",
                borderRadius: 10,
                border: `1px solid ${theme.border}`,
                background: theme.cardBgSubtle,
                color: theme.textStrong,
              }}
            />
          </label>
          <ToggleRow
            label="Clear clipboard after paste"
            detail="When the clipboard was empty, remove the transcript so it does not linger in OS clipboard history."
            value={settings.clear_clipboard_after_paste}
            onChange={(v) => onChange({ ...settings, clear_clipboard_after_paste: v })}
          />
        </div>
      </Card>

      <CrashReportsCard
        settings={settings}
        onChange={onChange}
      />

      <DataBackupCard />

      <Card>
        <SectionTitle sub="Version, legal, and purchase links.">About</SectionTitle>
        <div style={{ color: theme.textMuted, fontSize: 13, marginBottom: 12 }}>
          Version {APP_VERSION}
        </div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 10, marginBottom: 12 }}>
          <a
            href="https://github.com/ch1kim0n1/WhimprFlow/blob/main/docs/legal/PRIVACY.md"
            target="_blank"
            rel="noreferrer"
            style={aboutLinkStyle()}
          >
            Privacy Policy
          </a>
          <a
            href="https://github.com/ch1kim0n1/WhimprFlow/blob/main/docs/legal/TERMS.md"
            target="_blank"
            rel="noreferrer"
            style={aboutLinkStyle()}
          >
            Terms of Service
          </a>
          <a
            href="https://github.com/ch1kim0n1/WhimprFlow/blob/main/docs/legal/EULA.md"
            target="_blank"
            rel="noreferrer"
            style={aboutLinkStyle()}
          >
            EULA
          </a>
          <a href="https://whimprflow.com/buy" target="_blank" rel="noreferrer" style={aboutLinkStyle()}>
            Buy
          </a>
          <a href="mailto:support@whimprflow.com" style={aboutLinkStyle()}>
            support@whimprflow.com
          </a>
        </div>
        <a
          href="https://github.com/ch1kim0n1/WhimprFlow"
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

function aboutLinkStyle(): React.CSSProperties {
  return {
    display: "inline-flex",
    alignItems: "center",
    borderRadius: 10,
    padding: "8px 12px",
    color: theme.textStrong,
    background: theme.cardBgSubtle,
    border: `1px solid ${theme.border}`,
    textDecoration: "none",
    fontSize: 13,
    fontWeight: 600,
  };
}

function backupLabel(path: string): string {
  const base = path.replace(/\\/g, "/").split("/").pop() ?? path;
  const secs = Number(base);
  if (!Number.isFinite(secs) || secs <= 0) return base;
  try {
    return new Date(secs * 1000).toLocaleString();
  } catch {
    return base;
  }
}

function CrashReportsCard({
  settings,
  onChange,
}: {
  settings: Settings;
  onChange: (s: Settings) => void;
}) {
  const [paths, setPaths] = useState<string[]>([]);
  useEffect(() => {
    void listCrashReports().then(setPaths);
  }, [settings.crash_reporting_opt_in]);
  return (
    <Card style={{ marginBottom: 16 }}>
      <SectionTitle
        icon="shield"
        sub="Crash reports stay on this device unless you export and share them."
      >
        Crash reports
      </SectionTitle>
      <ToggleRow
        label="Write local crash reports on panic"
        detail="Off by default. When on, panics write crash-<timestamp>.txt under the app support folder."
        value={settings.crash_reporting_opt_in}
        onChange={(v) => onChange({ ...settings, crash_reporting_opt_in: v })}
      />
      {paths.length === 0 ? (
        <div style={{ color: theme.textMuted, fontSize: 13, marginTop: 10 }}>No crash reports yet.</div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 8, marginTop: 12 }}>
          {paths.slice(0, 8).map((p) => (
            <div
              key={p}
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                gap: 12,
                fontSize: 12.5,
                color: theme.textMuted,
              }}
            >
              <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{p}</span>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  void exportDiagnostics();
                }}
              >
                Export
              </Button>
            </div>
          ))}
        </div>
      )}
    </Card>
  );
}

function DataBackupCard() {
  const [paths, setPaths] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  const refresh = () => void listBackups().then(setPaths);

  useEffect(() => {
    refresh();
  }, []);

  const onDiagnostics = async () => {
    setBusy(true);
    setMsg(null);
    try {
      const path = await exportDiagnostics();
      setMsg(`Diagnostics saved to ${path}`);
    } catch (e) {
      setMsg(String(e));
    } finally {
      setBusy(false);
    }
  };

  const onBackup = async () => {
    setBusy(true);
    setMsg(null);
    try {
      const dest = await backupData();
      setMsg(`Saved to ${dest}`);
      refresh();
    } catch (e) {
      setMsg(String(e));
    } finally {
      setBusy(false);
    }
  };

  const onRestore = async (path: string) => {
    if (!window.confirm("Restore this backup? Current settings, dictionary, snippets, and workflows will be overwritten.")) {
      return;
    }
    setBusy(true);
    setMsg(null);
    try {
      const n = await restoreBackup(path);
      setMsg(`Restored ${n} file${n === 1 ? "" : "s"}. Quit and reopen if anything looks stale.`);
    } catch (e) {
      setMsg(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card style={{ marginBottom: 16 }}>
      <SectionTitle
        icon="archive"
        sub="Copy settings, dictionary, snippets, workflows, and history into a timestamped folder. Keeps the newest 20 backups."
      >
        Data backup
      </SectionTitle>
      <div style={{ display: "flex", flexWrap: "wrap", gap: 10, marginBottom: 14 }}>
        <Button variant="accent" size="sm" disabled={busy} onClick={() => void onBackup()}>
          {busy ? "Working…" : "Back up now"}
        </Button>
        <Button variant="ghost" size="sm" disabled={busy} onClick={() => void onDiagnostics()}>
          Export diagnostics
        </Button>
      </div>
      {paths.length === 0 ? (
        <div style={{ color: theme.textMuted, fontSize: 13 }}>No backups yet.</div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          {paths.slice(0, 8).map((p) => (
            <div
              key={p}
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                gap: 12,
                padding: "8px 10px",
                borderRadius: 10,
                background: theme.cardBgSubtle,
                border: `1px solid ${theme.border}`,
              }}
            >
              <div style={{ fontSize: 13, color: theme.textStrong, fontWeight: 550 }}>{backupLabel(p)}</div>
              <Button variant="ghost" size="sm" disabled={busy} onClick={() => void onRestore(p)}>
                Restore
              </Button>
            </div>
          ))}
        </div>
      )}
      {msg && (
        <div style={{ color: theme.textMuted, fontSize: 12.5, marginTop: 12, lineHeight: 1.4 }}>{msg}</div>
      )}
    </Card>
  );
}
