import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Button, Card, Segmented } from "./ui";
import { Icon } from "./icons";
import { dayLabel, fmtTimeOfDay } from "./format";
import {
  addWorkflow,
  approvePending,
  getPending,
  getWorkflows,
  onPending,
  rejectPending,
  removeWorkflow,
  type PendingEvent,
  type WorkflowDestination,
  type WorkflowEntry,
} from "./api";

// Voice Workflows: named spoken-trigger routines. Saying the trigger routes the
// rest of the utterance through the workflow's instruction and sends the result
// to its destination; approval-gated workflows hold here until reviewed.

const DESTINATIONS: { value: WorkflowDestination; label: string }[] = [
  { value: "paste", label: "Paste at cursor" },
  { value: "clipboard", label: "Copy to clipboard" },
  { value: "note", label: "Save to Studio notes" },
];

function destinationLabel(d: WorkflowDestination): string {
  return DESTINATIONS.find((x) => x.value === d)?.label ?? d;
}

// "Today, 9:41 am" / "Jul 15, 9:41 am" for revision timestamps.
function fmtStamp(tsUnix: number): string {
  const d = new Date(tsUnix * 1000);
  return `${dayLabel(d)}, ${fmtTimeOfDay(d)}`;
}

function Chip({ children }: { children: ReactNode }) {
  return (
    <span
      style={{
        fontSize: 11.5,
        fontWeight: 600,
        color: theme.textMuted,
        background: theme.track,
        borderRadius: 6,
        padding: "2px 7px",
        whiteSpace: "nowrap",
      }}
    >
      {children}
    </span>
  );
}

// A workflow result held for review (the "whimpr://pending" event). Approve
// delivers it to the workflow's destination; Reject discards it.
function PendingCard({ pending, onClear }: { pending: PendingEvent; onClear: () => void }) {
  const act = async (approve: boolean) => {
    if (approve) await approvePending();
    else await rejectPending();
    onClear();
  };
  return (
    // role="status" so the held result is announced when it appears.
    <div role="status">
      <Card style={{ marginBottom: 16, borderColor: theme.accentSoftBorder }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 10 }}>
          <Icon name="lock" size={16} style={{ color: theme.accentDeep }} />
          <span style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>
            Waiting for your approval
          </span>
          <Chip>{pending.name}</Chip>
        </div>
        <div
          style={{
            background: theme.cardBgSubtle,
            border: `1px solid ${theme.border}`,
            borderRadius: 10,
            padding: "10px 12px",
            fontSize: 13,
            color: theme.textBody,
            whiteSpace: "pre-wrap",
            marginBottom: 12,
          }}
        >
          {pending.preview}
        </div>
        <div style={{ display: "flex", gap: 8 }}>
          <Button variant="accent" onClick={() => void act(true)}>
            <Icon name="check" size={15} style={{ color: "#fff" }} />
            Approve
          </Button>
          <Button variant="ghost" onClick={() => void act(false)}>
            Reject
          </Button>
        </div>
      </Card>
    </div>
  );
}

// Add/edit form. Saving upserts by name on the backend: an edit bumps the
// version and archives the prior revision, so name stays fixed while editing.
function EditForm({ initial, onDone }: { initial: WorkflowEntry | null; onDone: () => void }) {
  const [name, setName] = useState(initial?.name ?? "");
  const [trigger, setTrigger] = useState(initial?.trigger ?? "");
  const [instruction, setInstruction] = useState(initial?.instruction ?? "");
  const [destination, setDestination] = useState<WorkflowDestination>(initial?.destination ?? "paste");
  const [requireApproval, setRequireApproval] = useState(initial?.require_approval ?? false);
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
  const labelStyle = { fontSize: 12, color: theme.textMuted, display: "block", marginBottom: 5 } as const;

  const ready = name.trim() && trigger.trim() && instruction.trim();
  const submit = async () => {
    if (!ready) return;
    await addWorkflow(name.trim(), trigger.trim(), instruction.trim(), destination, requireApproval);
    onDone();
  };

  return (
    <Card style={{ marginBottom: 16, borderColor: theme.accentSoftBorder }}>
      <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong, marginBottom: 12 }}>
        {initial ? `Edit "${initial.name}"` : "Add a workflow"}
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        <div>
          <label style={labelStyle}>Name</label>
          <input
            autoFocus={!initial}
            value={name}
            disabled={!!initial}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. Jira ticket"
            style={{ ...inputStyle, opacity: initial ? 0.6 : 1 }}
          />
        </div>
        <div>
          <label style={labelStyle}>Trigger phrase</label>
          <input
            autoFocus={!!initial}
            value={trigger}
            onChange={(e) => setTrigger(e.target.value)}
            placeholder="e.g. jira this"
            style={inputStyle}
          />
        </div>
        <div>
          <label style={labelStyle}>Instruction</label>
          <textarea
            value={instruction}
            onChange={(e) => setInstruction(e.target.value)}
            placeholder="Rewrite the text as a Jira ticket: a one-line summary, then steps to reproduce."
            rows={3}
            style={inputStyle}
          />
        </div>
        <div>
          <label style={labelStyle}>Destination</label>
          <select
            value={destination}
            onChange={(e) => setDestination(e.target.value as WorkflowDestination)}
            aria-label="Workflow destination"
            style={{ ...inputStyle, width: undefined, minWidth: 220 }}
          >
            {DESTINATIONS.map((d) => (
              <option key={d.value} value={d.value}>
                {d.label}
              </option>
            ))}
          </select>
        </div>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 16 }}>
          <div>
            <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>Hold for approval</div>
            <div style={{ fontSize: 12.5, color: theme.textMuted, marginTop: 2, lineHeight: 1.45, maxWidth: 430 }}>
              Review the result here before it is delivered anywhere.
            </div>
          </div>
          <Segmented
            options={[
              { value: "on", label: "On" },
              { value: "off", label: "Off" },
            ]}
            value={requireApproval ? "on" : "off"}
            onChange={(v) => setRequireApproval(v === "on")}
          />
        </div>
      </div>
      <div style={{ display: "flex", gap: 8, marginTop: 14 }}>
        <Button variant="accent" disabled={!ready} onClick={() => void submit()}>
          {initial ? "Save changes" : "Add workflow"}
        </Button>
        <Button variant="ghost" onClick={onDone}>
          Cancel
        </Button>
      </div>
    </Card>
  );
}

function EntryRow({
  entry,
  onEdit,
  onRemove,
}: {
  entry: WorkflowEntry;
  onEdit: () => void;
  onRemove: () => void;
}) {
  const [hover, setHover] = useState(false);
  const [showHistory, setShowHistory] = useState(false);
  const revisions = [...entry.history].sort((a, b) => b.version - a.version);
  const iconButtonStyle = {
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
  } as const;
  return (
    <div
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{ padding: "12px 6px", borderBottom: `1px solid ${theme.border}` }}
    >
      <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between", gap: 12 }}>
        <div style={{ minWidth: 0 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
            <span style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>{entry.name}</span>
            <span style={{ fontFamily: font.mono, fontSize: 12, color: theme.accentDeep }}>
              "{entry.trigger} ..."
            </span>
            <Chip>{destinationLabel(entry.destination)}</Chip>
            {entry.require_approval && <Chip>Approval hold</Chip>}
            <Chip>v{entry.version}</Chip>
          </div>
          <div
            style={{
              marginTop: 3,
              fontSize: 12.5,
              color: theme.textMuted,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {entry.instruction}
          </div>
          {revisions.length > 0 && (
            <button
              onClick={() => setShowHistory((s) => !s)}
              aria-expanded={showHistory}
              style={{
                border: "none",
                background: "transparent",
                cursor: "pointer",
                padding: 0,
                marginTop: 6,
                fontFamily: font.ui,
                fontSize: 12,
                fontWeight: 600,
                color: theme.textFaint,
              }}
            >
              {showHistory ? "Hide version history" : `Version history (${revisions.length})`}
            </button>
          )}
        </div>
        <div style={{ display: "flex", gap: 2 }}>
          <button onClick={onEdit} title="Edit" aria-label={`Edit ${entry.name}`} style={iconButtonStyle}>
            <Icon name="scratchpad" size={16} />
          </button>
          <button onClick={onRemove} title="Delete" aria-label={`Delete ${entry.name}`} style={iconButtonStyle}>
            <Icon name="close" size={16} />
          </button>
        </div>
      </div>
      {showHistory && (
        <div style={{ marginTop: 8, borderLeft: `2px solid ${theme.border}`, paddingLeft: 10 }}>
          {revisions.map((r) => (
            <div key={r.version} style={{ display: "flex", alignItems: "baseline", gap: 8, padding: "3px 0", minWidth: 0 }}>
              <span style={{ fontFamily: font.mono, fontSize: 11.5, color: theme.textFaint, flex: "0 0 auto" }}>
                v{r.version}
              </span>
              <span style={{ fontSize: 11.5, color: theme.textFaint, flex: "0 0 auto" }}>{fmtStamp(r.updated_unix)}</span>
              <span
                style={{
                  fontSize: 12,
                  color: theme.textMuted,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {r.instruction}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export function WorkflowsPane() {
  const [entries, setEntries] = useState<WorkflowEntry[]>([]);
  // null = form closed; an entry = editing it; "new" = blank add form.
  const [editing, setEditing] = useState<WorkflowEntry | "new" | null>(null);
  const [pending, setPending] = useState<PendingEvent | null>(null);

  const load = () => getWorkflows().then(setEntries);
  useEffect(() => {
    void load();
  }, []);

  // A workflow result held for approval. Seed from the shell's held slot on
  // mount - the "whimpr://pending" event is fire-and-forget, so a result
  // raised while this pane wasn't mounted is only reachable by asking - then
  // stay live via the event. The null-guard keeps a slow empty query from
  // clobbering a card an event already put up.
  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    void getPending().then((p) => {
      if (alive && p) setPending(p);
    });
    void onPending((p) => setPending(p)).then((u) => {
      if (alive) unlisten = u;
      else u();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  const remove = async (name: string) => {
    await removeWorkflow(name);
    await load();
  };

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
            Workflows
          </h1>
          <p style={{ color: theme.textMuted, fontSize: 14, lineHeight: 1.5, margin: "8px 0 0", maxWidth: 520 }}>
            Say a trigger phrase, then your content. Saying "jira this the login button ignores dark
            mode" runs the "jira this" workflow: its instruction rewrites the rest of the sentence
            and sends the result to the destination you pick.
          </p>
        </div>
        <Button variant="accent" onClick={() => setEditing((e) => (e ? null : "new"))}>
          <Icon name="plus" size={15} style={{ color: "#fff" }} />
          Add new
        </Button>
      </div>

      {pending && <PendingCard pending={pending} onClear={() => setPending(null)} />}

      {editing && (
        <EditForm
          initial={editing === "new" ? null : editing}
          onDone={() => {
            setEditing(null);
            void load();
          }}
        />
      )}

      <Card pad={entries.length ? 8 : 22}>
        {entries.length === 0 ? (
          <div style={{ padding: "30px 8px", textAlign: "center", color: theme.textFaint, fontSize: 13.5 }}>
            No workflows yet. Add one to turn a spoken trigger into a routine: rewrite, then paste,
            copy, or save to Studio notes.
          </div>
        ) : (
          <div style={{ padding: "4px 14px" }}>
            {entries.map((e) => (
              <EntryRow
                key={e.name}
                entry={e}
                onEdit={() => setEditing(e)}
                onRemove={() => void remove(e.name)}
              />
            ))}
          </div>
        )}
      </Card>
    </div>
  );
}
