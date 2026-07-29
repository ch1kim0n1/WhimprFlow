import { useEffect, useState } from "react";
import { theme } from "./theme";
import { Button, Card, PageTitle } from "./ui";
import {
  activateLicense,
  clearLicense,
  getEntitlement,
  startTrial,
  type Entitlement,
} from "./api";

export function LicensePane() {
  const [ent, setEnt] = useState<Entitlement | null>(null);
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  const refresh = () => void getEntitlement().then(setEnt);

  useEffect(() => {
    refresh();
  }, []);

  const onActivate = async () => {
    setBusy(true);
    setMsg(null);
    try {
      const next = await activateLicense(key.trim());
      setEnt(next);
      setKey("");
      setMsg(next.message);
    } catch (e) {
      setMsg(String(e));
    } finally {
      setBusy(false);
    }
  };

  const onTrial = async () => {
    setBusy(true);
    setMsg(null);
    try {
      const next = await startTrial();
      setEnt(next);
      setMsg(next.message);
    } catch (e) {
      setMsg(String(e));
    } finally {
      setBusy(false);
    }
  };

  const onClear = async () => {
    setBusy(true);
    setMsg(null);
    try {
      const next = await clearLicense();
      setEnt(next);
      setMsg("License removed from this device.");
    } catch (e) {
      setMsg(String(e));
    } finally {
      setBusy(false);
    }
  };

  const kind = ent?.kind ?? "unlicensed";
  const purchase = ent?.purchase_url ?? "https://whimprflow.com/buy";

  return (
    <div style={{ maxWidth: 640 }}>
      <PageTitle sub="One-time purchase unlocks cloud cleanup. Local transcription always works.">
        License
      </PageTitle>

      <Card style={{ marginBottom: 16 }}>
        <div style={{ fontSize: 14, fontWeight: 650, color: theme.textStrong, marginBottom: 8 }}>
          Status: {kind === "licensed" ? "Licensed" : kind === "trial" ? "Trial" : "Unlicensed"}
        </div>
        <div style={{ color: theme.textMuted, fontSize: 13, lineHeight: 1.45 }}>
          {ent?.message ?? "Loading…"}
        </div>
        {ent?.email && (
          <div style={{ color: theme.textMuted, fontSize: 13, marginTop: 8 }}>
            Registered to {ent.email}
          </div>
        )}
        {ent?.trial_days_remaining != null && (
          <div style={{ color: theme.textMuted, fontSize: 13, marginTop: 8 }}>
            Trial days remaining: {ent.trial_days_remaining}
          </div>
        )}
        <div style={{ marginTop: 14, color: theme.textMuted, fontSize: 12.5, lineHeight: 1.45 }}>
          Cloud cleanup (OpenAI / Anthropic) requires a license or trial. Raw and local cleanup stay
          available either way.
        </div>
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <div style={{ fontSize: 14, fontWeight: 650, color: theme.textStrong, marginBottom: 10 }}>
          Activate license key
        </div>
        <input
          value={key}
          onChange={(e) => setKey(e.target.value)}
          placeholder="WF1...."
          spellCheck={false}
          style={{
            width: "100%",
            boxSizing: "border-box",
            borderRadius: 10,
            border: `1px solid ${theme.border}`,
            background: theme.cardBgSubtle,
            color: theme.textStrong,
            padding: "10px 12px",
            fontSize: 13,
            fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
            marginBottom: 12,
          }}
        />
        <div style={{ display: "flex", flexWrap: "wrap", gap: 10 }}>
          <Button variant="accent" size="sm" disabled={busy || !key.trim()} onClick={() => void onActivate()}>
            Activate
          </Button>
          {kind === "licensed" && (
            <Button variant="ghost" size="sm" disabled={busy} onClick={() => void onClear()}>
              Remove license
            </Button>
          )}
        </div>
      </Card>

      <Card style={{ marginBottom: 16 }}>
        <div style={{ fontSize: 14, fontWeight: 650, color: theme.textStrong, marginBottom: 8 }}>
          Free trial
        </div>
        <div style={{ color: theme.textMuted, fontSize: 13, lineHeight: 1.45, marginBottom: 12 }}>
          14 days of full features, including cloud cleanup. Start date is stored in the OS keychain.
        </div>
        <Button
          variant="dark"
          size="sm"
          disabled={busy || kind !== "unlicensed"}
          onClick={() => void onTrial()}
        >
          Start 14-day trial
        </Button>
      </Card>

      <Card>
        <div style={{ fontSize: 14, fontWeight: 650, color: theme.textStrong, marginBottom: 8 }}>
          Buy WhimprFlow
        </div>
        <div style={{ color: theme.textMuted, fontSize: 13, lineHeight: 1.45, marginBottom: 12 }}>
          Purchase a license, then paste the key you receive above. Taxes (including VAT) are handled
          by the payment provider on the checkout page.
        </div>
        <a
          href={purchase}
          target="_blank"
          rel="noreferrer"
          style={{
            display: "inline-flex",
            alignItems: "center",
            borderRadius: 10,
            padding: "10px 14px",
            background: theme.accentSoft,
            color: theme.accentDeep,
            textDecoration: "none",
            fontSize: 13.5,
            fontWeight: 650,
          }}
        >
          Buy license
        </a>
        {msg && (
          <div style={{ color: theme.textMuted, fontSize: 12.5, marginTop: 12, lineHeight: 1.4 }}>
            {msg}
          </div>
        )}
      </Card>
    </div>
  );
}
