import { font } from "../tokens/values";
import { theme } from "./theme";
import { Card, PageTitle, Segmented } from "./ui";
import { MAX_STYLE_INSTRUCTIONS_LEN, type Formality, type Settings } from "./api";

const FORMALITY_OPTIONS: { value: Formality; label: string; hint: string }[] = [
  { value: "casual", label: "Casual", hint: "Relaxed and conversational; contractions welcome." },
  { value: "neutral", label: "Neutral", hint: "No steering; cleaned up but true to how you said it. (Default)" },
  { value: "formal", label: "Formal", hint: "Professional and polished; no slang or contractions." },
];

export function StylePane({ settings, onChange }: { settings: Settings; onChange: (s: Settings) => void }) {
  const { style } = settings;
  const activeFormality = FORMALITY_OPTIONS.find((o) => o.value === style.formality);

  return (
    <div style={{ maxWidth: 720 }}>
      <PageTitle sub="Shapes how your cleaned-up dictation reads (tone and formality) and applies automatically to every dictation. It only changes presentation; it never invents words you didn't say.">
        Style
      </PageTitle>

      <Card style={{ marginBottom: 16 }}>
        <div style={{ fontSize: 15, fontWeight: 600, color: theme.textStrong, marginBottom: 12 }}>Formality</div>
        <Segmented
          options={FORMALITY_OPTIONS.map((o) => ({ value: o.value, label: o.label }))}
          value={style.formality}
          onChange={(v) => onChange({ ...settings, style: { ...settings.style, formality: v } })}
        />
        <div style={{ color: theme.textMuted, fontSize: 12.5, marginTop: 10 }}>{activeFormality?.hint}</div>
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <div style={{ fontSize: 15, fontWeight: 600, color: theme.textStrong, marginBottom: 4 }}>
          Style note (optional)
        </div>
        <div style={{ fontSize: 13, color: theme.textMuted, marginBottom: 10 }}>
          A free-text nudge layered on top of the formality above.
        </div>
        <textarea
          value={style.custom_instructions}
          maxLength={MAX_STYLE_INSTRUCTIONS_LEN}
          rows={4}
          placeholder="e.g. British spelling, no exclamation marks, keep it punchy"
          onChange={(e) =>
            onChange({ ...settings, style: { ...settings.style, custom_instructions: e.target.value } })
          }
          style={{
            width: "100%",
            background: theme.cardBgSubtle,
            border: `1px solid ${theme.border}`,
            borderRadius: 10,
            padding: "9px 12px",
            color: theme.textBody,
            fontFamily: font.ui,
            fontSize: 13,
            outline: "none",
            boxSizing: "border-box",
            resize: "vertical",
          }}
        />
        <div style={{ fontSize: 12, color: theme.textMuted, marginTop: 6, textAlign: "right" }}>
          {style.custom_instructions.length}/{MAX_STYLE_INSTRUCTIONS_LEN}
        </div>
      </Card>

      <div style={{ fontSize: 12.5, color: theme.textMuted }}>Changes save automatically.</div>
    </div>
  );
}
