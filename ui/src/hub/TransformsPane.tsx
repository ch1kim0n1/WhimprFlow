import { useState } from "react";
import { font } from "../tokens/values";
import { runTransform } from "./api";
import { theme } from "./theme";
import { Button, Card, PageTitle } from "./ui";

// A Transform turns spoken/typed text into a shaped output by running a canned
// instruction through the configured cleanup provider (reusing the Command Mode
// path). The model preserves facts and won't invent content it wasn't given.
const TRANSFORMS: { id: string; label: string; desc: string; instruction: string }[] = [
  {
    id: "email",
    label: "Polished email",
    desc: "A clear, professional email; facts kept intact.",
    instruction:
      "Rewrite this as a clear, well-structured, professional email. Preserve every fact, name, number and date. Only include a greeting or sign-off if one is already present.",
  },
  {
    id: "summary",
    label: "Summary",
    desc: "A few tight sentences capturing the key facts.",
    instruction: "Summarize this concisely in a few sentences, preserving the key facts.",
  },
  {
    id: "bullets",
    label: "Bullet points",
    desc: "One idea per bullet, nothing added.",
    instruction: "Rewrite this as a tight bulleted list, one idea per bullet. Preserve every fact.",
  },
  {
    id: "concise",
    label: "Make concise",
    desc: "Shorter, same facts and meaning.",
    instruction: "Make this significantly more concise while preserving all facts and meaning.",
  },
  {
    id: "professional",
    label: "Professional tone",
    desc: "A polished, professional voice.",
    instruction: "Rewrite this in a polished, professional tone. Preserve all facts and meaning.",
  },
  {
    id: "grammar",
    label: "Fix grammar only",
    desc: "Grammar, spelling and punctuation; wording left alone.",
    instruction:
      "Fix only grammar, spelling and punctuation. Do not change wording, tone, or meaning otherwise.",
  },
];

const inputStyle = {
  width: "100%",
  background: theme.cardBgSubtle,
  border: `1px solid ${theme.border}`,
  borderRadius: 10,
  padding: "9px 12px",
  color: theme.textBody,
  fontFamily: font.ui,
  fontSize: 13,
  outline: "none",
  boxSizing: "border-box" as const,
  resize: "vertical" as const,
};

export function TransformsPane() {
  const [input, setInput] = useState("");
  const [customInstruction, setCustomInstruction] = useState("");
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [runningId, setRunningId] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const run = async (id: string, instruction: string) => {
    setBusy(true);
    setRunningId(id);
    setResult(null);
    setError(null);
    setCopied(false);
    try {
      const out = await runTransform(input, instruction);
      setResult(out);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      setRunningId(null);
    }
  };

  const copy = async () => {
    if (!result) return;
    await navigator.clipboard.writeText(result);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  const noInput = !input.trim();

  return (
    <div style={{ maxWidth: 720 }}>
      <PageTitle sub="Turn a rough thought into a finished output (an email, a summary, a to-do) in one click. Type below, or focus the box, hold Fn and speak to dictate straight in.">
        Transforms
      </PageTitle>

      <Card style={{ marginBottom: 16 }}>
        <div style={{ fontSize: 12, color: theme.textMuted, marginBottom: 5 }}>Your text</div>
        <textarea
          value={input}
          onChange={(e) => setInput(e.target.value)}
          rows={5}
          placeholder="Paste or dictate the text you want to reshape..."
          style={inputStyle}
        />

        <div style={{ display: "flex", flexWrap: "wrap", gap: 8, marginTop: 14 }}>
          {TRANSFORMS.map((t) => (
            <span key={t.id} title={t.desc}>
              <Button onClick={() => run(t.id, t.instruction)} disabled={busy || noInput}>
                {runningId === t.id ? "Running..." : t.label}
              </Button>
            </span>
          ))}
        </div>

        <div style={{ marginTop: 16 }}>
          <div style={{ fontSize: 12, color: theme.textMuted, marginBottom: 5 }}>Custom instruction</div>
          <div style={{ display: "flex", gap: 8 }}>
            <input
              value={customInstruction}
              onChange={(e) => setCustomInstruction(e.target.value)}
              placeholder="e.g. rewrite this as a friendly text message"
              style={inputStyle}
            />
            <Button
              onClick={() => run("custom", customInstruction)}
              disabled={busy || noInput || !customInstruction.trim()}
            >
              {runningId === "custom" ? "Running..." : "Run"}
            </Button>
          </div>
        </div>
      </Card>

      {error !== null && (
        <Card style={{ marginBottom: 16 }}>
          <div style={{ fontSize: 13, color: "#e5484d", whiteSpace: "pre-wrap" }}>{error}</div>
        </Card>
      )}

      {result !== null && (
        <Card>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              marginBottom: 10,
            }}
          >
            <div style={{ fontSize: 15, fontWeight: 600, color: theme.textStrong }}>Result</div>
            <Button variant="ghost" size="sm" onClick={copy}>
              {copied ? "Copied" : "Copy"}
            </Button>
          </div>
          <div
            style={{
              fontSize: 13,
              color: theme.textBody,
              background: theme.cardBgSubtle,
              border: `1px solid ${theme.border}`,
              borderRadius: 10,
              padding: "9px 12px",
              whiteSpace: "pre-wrap",
            }}
          >
            {result}
          </div>
        </Card>
      )}
    </div>
  );
}
