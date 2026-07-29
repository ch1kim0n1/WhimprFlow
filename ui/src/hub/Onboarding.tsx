import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { font } from "../tokens/values";
import { theme } from "./theme";
import {
  activateLicense,
  asrModelInstalled,
  downloadAsrModel,
  getEntitlement,
  micSelfTest,
  requestAccessibility,
  requestMicrophone,
  requestInputMonitoring,
  startTrial,
  type Entitlement,
  type ModelProgress,
  type Status,
} from "./api";
import { detectPlatformSync } from "./platform";

// A blocking gate: Accessibility + Microphone + a Whisper model. Permissions
// flip live as the OS applies them; the model step streams download progress.

function Step({
  n,
  title,
  detail,
  done,
  active,
  locked,
  required,
  actionLabel,
  onGrant,
}: {
  n: number;
  title: string;
  detail: string;
  done: boolean;
  active: boolean;
  locked: boolean;
  required: boolean;
  actionLabel?: string;
  onGrant: () => void;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 16,
        padding: "16px 18px",
        borderRadius: 14,
        marginBottom: 12,
        background: active ? theme.accentSoft : theme.cardBg,
        border: `1px solid ${active ? theme.accentSoftBorder : theme.border}`,
        boxShadow: theme.shadowSoft,
        opacity: locked ? 0.5 : 1,
      }}
    >
      <div
        style={{
          flex: "0 0 auto",
          width: 30,
          height: 30,
          borderRadius: 9999,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          fontWeight: 700,
          fontSize: 14,
          color: done ? "#fff" : theme.textMuted,
          background: done ? theme.accentDeep : theme.track,
        }}
      >
        {done ? "\u2713" : n}
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 15, fontWeight: 600, color: theme.textStrong }}>
          {title}{" "}
          <span style={{ fontSize: 12, color: theme.textFaint, fontWeight: 400 }}>
            {required ? "· required" : "· optional"}
          </span>
        </div>
        <div style={{ fontSize: 13, color: theme.textMuted, marginTop: 2 }}>{detail}</div>
      </div>
      {done ? (
        <span style={{ color: theme.accentDeep, fontSize: 13, fontWeight: 600 }}>Ready</span>
      ) : (
        <button
          onClick={onGrant}
          disabled={locked}
          style={{
            cursor: locked ? "default" : "pointer",
            border: "none",
            borderRadius: 10,
            padding: "9px 16px",
            fontSize: 13,
            fontWeight: 600,
            fontFamily: font.ui,
            color: theme.solidText,
            background: locked ? theme.textFaint : theme.solidBg,
            whiteSpace: "nowrap",
          }}
        >
          {actionLabel ?? "Grant"}
        </button>
      )}
    </div>
  );
}

const btnSecondary: React.CSSProperties = {
  cursor: "pointer",
  border: `1px solid ${theme.border}`,
  borderRadius: 10,
  padding: "9px 14px",
  fontSize: 13,
  fontWeight: 600,
  fontFamily: font.ui,
  color: theme.textStrong,
  background: theme.cardBgSubtle,
};

const btnPrimary: React.CSSProperties = {
  cursor: "pointer",
  border: "none",
  borderRadius: 10,
  padding: "9px 14px",
  fontSize: 13,
  fontWeight: 600,
  fontFamily: font.ui,
  color: theme.solidText,
  background: theme.solidBg,
};

export function Onboarding({
  status,
  refresh,
  onEnter,
}: {
  status: Status;
  refresh: () => void;
  onEnter: () => void;
}) {
  const isWindows = detectPlatformSync() === "windows";
  const [modelOk, setModelOk] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState<ModelProgress | null>(null);
  const [modelErr, setModelErr] = useState<string | null>(null);

  const [micTesting, setMicTesting] = useState(false);
  const [micPeak, setMicPeak] = useState<number | null>(null);
  const [micErr, setMicErr] = useState<string | null>(null);
  const [micSkipped, setMicSkipped] = useState(false);

  const [entitlement, setEntitlement] = useState<Entitlement | null>(null);
  const [trialBusy, setTrialBusy] = useState(false);
  const [licenseKey, setLicenseKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [licenseErr, setLicenseErr] = useState<string | null>(null);
  const [licenseBusy, setLicenseBusy] = useState(false);

  useEffect(() => {
    const id = setInterval(refresh, 1200);
    return () => clearInterval(id);
  }, [refresh]);

  useEffect(() => {
    void asrModelInstalled().then(setModelOk);
    void getEntitlement().then(setEntitlement);
  }, []);

  useEffect(() => {
    let un: (() => void) | undefined;
    void listen<ModelProgress>("whimpr://model/progress", (e) => {
      setProgress(e.payload);
    }).then((fn) => {
      un = fn;
    });
    return () => {
      un?.();
    };
  }, []);

  const acc = status.accessibility;
  const mic = status.microphone;
  const inp = status.input_monitoring;
  const micOk = micSkipped || (micPeak != null && micPeak > 0.01);
  const licensed = entitlement?.kind === "licensed" || entitlement?.kind === "trial";
  const canEnter = acc && mic && modelOk && (micOk || micSkipped);

  const startDownload = async () => {
    setDownloading(true);
    setModelErr(null);
    try {
      await downloadAsrModel("base.en");
      setModelOk(true);
    } catch (e) {
      setModelErr(e instanceof Error ? e.message : String(e));
    } finally {
      setDownloading(false);
    }
  };

  const runMicTest = async () => {
    setMicTesting(true);
    setMicErr(null);
    setMicPeak(null);
    try {
      const peak = await micSelfTest();
      setMicPeak(peak);
      setMicSkipped(false);
    } catch (e) {
      setMicErr(e instanceof Error ? e.message : String(e));
    } finally {
      setMicTesting(false);
    }
  };

  const onStartTrial = async () => {
    setTrialBusy(true);
    try {
      const ent = await startTrial();
      setEntitlement(ent);
    } catch (e) {
      setLicenseErr(e instanceof Error ? e.message : String(e));
    } finally {
      setTrialBusy(false);
    }
  };

  const onActivate = async () => {
    setLicenseBusy(true);
    setLicenseErr(null);
    try {
      const ent = await activateLicense(licenseKey.trim());
      setEntitlement(ent);
    } catch (e) {
      setLicenseErr(e instanceof Error ? e.message : String(e));
    } finally {
      setLicenseBusy(false);
    }
  };

  const pct =
    progress && progress.total > 0
      ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100))
      : null;

  const micStepN = isWindows ? 3 : 4;
  const modelStepN = micStepN + 1;
  const readyStepN = modelStepN + 1;
  const peakPct = micPeak != null ? Math.min(100, Math.round(micPeak * 1000) / 10) : null;
  const peakGood = micPeak != null && micPeak > 0.01;

  return (
    <div
      style={{
        height: "100vh",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: theme.pageBg,
        color: theme.textBody,
        fontFamily: font.ui,
        padding: 24,
        overflowY: "auto",
      }}
    >
      <div style={{ width: 560, maxWidth: "100%", padding: "12px 0" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 6 }}>
          <div style={{ fontFamily: font.serif, fontSize: 30, fontWeight: 600, color: theme.textStrong }}>
            Set up WhimprFlow
          </div>
          <span
            style={{
              fontSize: 10,
              fontWeight: 700,
              letterSpacing: 0.4,
              textTransform: "uppercase",
              color: theme.accentDeep,
              background: theme.accentSoft,
              border: `1px solid ${theme.accentSoftBorder}`,
              borderRadius: 999,
              padding: "2px 7px",
            }}
          >
            Local
          </span>
        </div>
        <p style={{ color: theme.textMuted, lineHeight: 1.5, margin: "0 0 16px" }}>
          {isWindows
            ? "Allow microphone access, confirm paste works in other apps, and install the speech model. Hold Right Ctrl to dictate."
            : "Grant these to WhimprFlow, in order. Each turns green the moment the OS applies it. Hold Fn to dictate."}
        </p>

        {(() => {
          const names = ["Permissions", "Microphone test", "Speech model", "Trial/License", "Ready"] as const;
          const idx = !acc || !mic ? 0 : !micOk ? 1 : !modelOk ? 2 : !licensed ? 3 : 4;
          return (
            <div style={{ marginBottom: 20 }} aria-label={`Onboarding step ${idx + 1} of 5`}>
              <div style={{ display: "flex", justifyContent: "space-between", marginBottom: 6, fontSize: 12, color: theme.textFaint }}>
                <span>
                  Step {idx + 1}/5 · {names[idx]}
                </span>
              </div>
              <div style={{ height: 6, borderRadius: 999, background: theme.track, overflow: "hidden" }}>
                <div
                  style={{
                    height: "100%",
                    width: `${((idx + 1) / 5) * 100}%`,
                    background: theme.accentDeep,
                    transition: "width 0.25s ease",
                  }}
                />
              </div>
            </div>
          );
        })()}

        <Step
          n={1}
          title={isWindows ? "Microphone" : "Accessibility"}
          detail={
            isWindows
              ? "Lets WhimprFlow hear what you say. Windows will prompt the first time you record."
              : "Detects the Fn key in every app and types your words."
          }
          done={isWindows ? mic : acc}
          active={isWindows ? !mic : !acc}
          locked={false}
          required
          onGrant={() => (isWindows ? requestMicrophone() : requestAccessibility())}
        />
        <Step
          n={2}
          title={isWindows ? "Accessibility / paste" : "Microphone"}
          detail={
            isWindows
              ? "No special Accessibility toggle on Windows. Open privacy settings if paste into other apps is blocked."
              : "Lets WhimprFlow hear what you say."
          }
          done={isWindows ? acc : mic}
          active={isWindows ? mic && !acc : acc && !mic}
          locked={isWindows ? !mic : !acc}
          required
          onGrant={() => (isWindows ? requestAccessibility() : requestMicrophone())}
        />
        {!isWindows && (
          <Step
            n={3}
            title="Input Monitoring"
            detail="Extra reliability for key detection. Optional; you can enter without it."
            done={inp}
            active={acc && mic && !inp}
            locked={!(acc && mic)}
            required={false}
            onGrant={() => requestInputMonitoring()}
          />
        )}

        <div
          style={{
            padding: "16px 18px",
            borderRadius: 14,
            marginBottom: 12,
            background: theme.cardBg,
            border: `1px solid ${theme.border}`,
            boxShadow: theme.shadowSoft,
            opacity: !(acc && mic) ? 0.5 : 1,
          }}
        >
          <div style={{ fontSize: 15, fontWeight: 600, color: theme.textStrong, marginBottom: 4 }}>
            {micStepN}. Test your microphone{" "}
            <span style={{ fontSize: 12, color: theme.textFaint, fontWeight: 400 }}>· recommended</span>
          </div>
          <div style={{ fontSize: 13, color: theme.textMuted, marginBottom: 12 }}>
            Captures 2 seconds from the default input and reports peak level.
          </div>
          {micTesting && (
            <div style={{ fontSize: 13, color: theme.textMuted, marginBottom: 10 }}>Listening…</div>
          )}
          {micErr && (
            <div style={{ fontSize: 13, color: "#b42318", marginBottom: 10 }}>{micErr}</div>
          )}
          {peakPct != null && !micErr && (
            <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 10, fontSize: 13.5 }}>
              <span
                style={{
                  width: 10,
                  height: 10,
                  borderRadius: 999,
                  background: peakGood ? "#3daa6d" : "#b42318",
                }}
              />
              Peak RMS ≈ {peakPct}% {peakGood ? "(good)" : "(silent — check input device)"}
            </div>
          )}
          {micSkipped && (
            <div style={{ fontSize: 13, color: theme.textMuted, marginBottom: 10 }}>Mic test skipped.</div>
          )}
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            <button type="button" style={btnPrimary} disabled={!(acc && mic) || micTesting} onClick={() => void runMicTest()}>
              {micErr ? "Retry" : micTesting ? "Testing…" : "Run mic test"}
            </button>
            <button
              type="button"
              style={btnSecondary}
              disabled={!(acc && mic) || micTesting}
              onClick={() => {
                setMicSkipped(true);
                setMicErr(null);
              }}
            >
              Skip
            </button>
          </div>
        </div>

        <Step
          n={modelStepN}
          title="Speech model"
          detail={
            downloading && pct != null
              ? `Downloading Whisper base (English)... ${pct}%`
              : modelErr
                ? modelErr
                : "About 140 MB. Required for local transcription. Verified with SHA-256."
          }
          done={modelOk}
          active={!modelOk && micOk}
          locked={downloading || !micOk}
          required
          actionLabel={downloading ? "Downloading..." : "Download"}
          onGrant={() => {
            if (!downloading) void startDownload();
          }}
        />

        <div
          style={{
            padding: "16px 18px",
            borderRadius: 14,
            marginBottom: 12,
            background: theme.cardBg,
            border: `1px solid ${theme.border}`,
            boxShadow: theme.shadowSoft,
            opacity: !(acc && mic && modelOk && micOk) ? 0.55 : 1,
          }}
        >
          <div style={{ fontSize: 15, fontWeight: 600, color: theme.textStrong, marginBottom: 4 }}>
            {readyStepN}. You&apos;re ready to dictate
          </div>
          <div style={{ fontSize: 13, color: theme.textMuted, marginBottom: 12, lineHeight: 1.45 }}>
            WhimprFlow includes 14 days of cloud cleanup (OpenAI/Anthropic). Local transcription is always free.
          </div>
          {licensed ? (
            <div style={{ fontSize: 13.5, fontWeight: 600, color: theme.accentDeep, marginBottom: 10 }}>
              {entitlement?.kind === "licensed"
                ? "License active — cloud cleanup unlocked"
                : `License active — trial (${entitlement?.trial_days_remaining ?? "?"} days left)`}
            </div>
          ) : (
            <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginBottom: 12 }}>
              <button type="button" style={btnPrimary} disabled={trialBusy} onClick={() => void onStartTrial()}>
                {trialBusy ? "Starting…" : "Start 14-day trial"}
              </button>
              <button type="button" style={btnSecondary} onClick={() => setShowKey((v) => !v)}>
                I have a license key
              </button>
            </div>
          )}
          {showKey && !licensed && (
            <div style={{ marginBottom: 10 }}>
              <input
                value={licenseKey}
                onChange={(e) => setLicenseKey(e.target.value)}
                placeholder="Paste license key"
                style={{
                  width: "100%",
                  boxSizing: "border-box",
                  marginBottom: 8,
                  padding: "9px 12px",
                  borderRadius: 10,
                  border: `1px solid ${theme.border}`,
                  background: theme.cardBgSubtle,
                  fontFamily: font.ui,
                  fontSize: 13,
                  color: theme.textBody,
                }}
              />
              <button
                type="button"
                style={btnPrimary}
                disabled={licenseBusy || !licenseKey.trim()}
                onClick={() => void onActivate()}
              >
                {licenseBusy ? "Activating…" : "Activate"}
              </button>
            </div>
          )}
          {licenseErr && (
            <div style={{ fontSize: 13, color: "#b42318", marginBottom: 8 }}>{licenseErr}</div>
          )}
        </div>

        <button
          onClick={onEnter}
          disabled={!canEnter}
          style={{
            marginTop: 4,
            width: "100%",
            cursor: canEnter ? "pointer" : "default",
            border: "none",
            borderRadius: 12,
            padding: "13px",
            fontSize: 15,
            fontWeight: 700,
            fontFamily: font.ui,
            color: "#fff",
            background: canEnter ? theme.accentDeep : theme.textFaint,
          }}
        >
          {canEnter
            ? licensed
              ? "Enter WhimprFlow"
              : "Enter WhimprFlow (skip trial for now)"
            : "Finish permissions, mic test (or skip), and model download"}
        </button>

        <p style={{ fontSize: 12, color: theme.textFaint, lineHeight: 1.5, marginTop: 16 }}>
          {isWindows
            ? "Push-to-talk is Right Ctrl. If the pill never reacts, run WhimprFlow as a normal (non-elevated) user and retry."
            : "If a permission stays grey after you flip it on in System Settings, toggle WhimprFlow off and back on in that pane."}
        </p>
      </div>
    </div>
  );
}
