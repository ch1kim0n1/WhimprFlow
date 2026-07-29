import { useEffect, useState } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Button, Card, PageTitle } from "./ui";
import { Icon, type IconName } from "./icons";
import { exportDiagnostics, getBuildInfo, type BuildInfo } from "./api";

const TIPS: { icon: IconName; title: string; body: string }[] = [
  {
    icon: "mic",
    title: "Hold to dictate",
    body: "Press and hold your dictation key, speak naturally, then release. WhimprFlow transcribes on-device. Nothing leaves your machine unless you choose a cloud cleanup engine and hold a license or trial.",
  },
  {
    icon: "sparkles",
    title: "Cleanup happens where your cursor is",
    body: "Release the key and your cleaned-up text is typed into the active app. Set the cleanup strength under Settings.",
  },
  {
    icon: "book",
    title: "Teach it your vocabulary",
    body: "Open Dictionary and add names, jargon, or acronyms it keeps mishearing.",
  },
  {
    icon: "lock",
    title: "Pick a cleanup engine",
    body: "Under Settings, run fully offline (Local), paste exactly what you said (Raw), or add an OpenAI / Anthropic key for cloud cleanup. Keys stay in the OS keychain.",
  },
];

export function Help() {
  const [build, setBuild] = useState<BuildInfo | null>(null);
  const [diag, setDiag] = useState<string | null>(null);

  useEffect(() => {
    void getBuildInfo().then(setBuild);
  }, []);

  return (
    <div style={{ maxWidth: 720 }}>
      <PageTitle sub="Tips, support, and diagnostics.">Help</PageTitle>

      <Card style={{ marginBottom: 14 }}>
        <div style={{ fontSize: 15, fontWeight: 650, color: theme.textStrong, marginBottom: 8 }}>
          Help & Support
        </div>
        <div style={{ color: theme.textMuted, fontSize: 13, lineHeight: 1.45, marginBottom: 12 }}>
          Version {build?.version ?? "…"} ({build?.git_hash ?? "…"}). Troubleshooting guide:{" "}
          <a
            href="https://github.com/ch1kim0n1/WhimprFlow/blob/main/docs/HELP.md"
            target="_blank"
            rel="noreferrer"
            style={{ color: theme.accentDeep }}
          >
            docs/HELP.md
          </a>
          .
        </div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 10 }}>
          <a
            href="mailto:support@whimprflow.com"
            style={{
              display: "inline-flex",
              padding: "8px 12px",
              borderRadius: 10,
              background: theme.cardBgSubtle,
              border: `1px solid ${theme.border}`,
              color: theme.textStrong,
              textDecoration: "none",
              fontSize: 13,
              fontWeight: 600,
            }}
          >
            support@whimprflow.com
          </a>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              void exportDiagnostics()
                .then((p) => setDiag(p))
                .catch((e) => setDiag(String(e)));
            }}
          >
            Export diagnostics
          </Button>
        </div>
        {diag && (
          <div style={{ marginTop: 10, fontSize: 12.5, color: theme.textMuted }}>{diag}</div>
        )}
      </Card>

      <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
        {TIPS.map((t) => (
          <Card key={t.title}>
            <div style={{ display: "flex", gap: 14 }}>
              <div
                style={{
                  width: 40,
                  height: 40,
                  borderRadius: 12,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  background: theme.accentSoft,
                  border: `1px solid ${theme.accentSoftBorder}`,
                  color: theme.accentDeep,
                  flex: "0 0 auto",
                }}
              >
                <Icon name={t.icon} size={18} strokeWidth={1.7} />
              </div>
              <div>
                <div
                  style={{
                    fontFamily: font.ui,
                    fontSize: 15,
                    fontWeight: 600,
                    color: theme.textStrong,
                    marginBottom: 4,
                  }}
                >
                  {t.title}
                </div>
                <div style={{ fontSize: 13.5, lineHeight: 1.55, color: theme.textMuted }}>{t.body}</div>
              </div>
            </div>
          </Card>
        ))}
      </div>
    </div>
  );
}
