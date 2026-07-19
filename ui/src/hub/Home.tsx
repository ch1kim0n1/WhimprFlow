import { useEffect, useState } from "react";
import { font, palette } from "../tokens/values";
import { theme } from "./theme";
import { Card, useStats } from "./ui";
import { Icon } from "./icons";
import { getHistory, type HistoryItem, type StatsSummary } from "./api";
import { dayKey, dayLabel, fmtCompact, fmtDuration, fmtNum, fmtTimeOfDay, wordsReference } from "./format";

const UNLOCK_WORDS = 500;

const HERO_MOTION_STYLES = `
  @keyframes home-hero-rise {
    from { opacity: 0; transform: translateY(22px); }
    to { opacity: 1; transform: translateY(0); }
  }
  @keyframes home-hero-drift-a {
    0%, 100% { transform: translate3d(0, 0, 0) scale(1); }
    50% { transform: translate3d(42px, -26px, 0) scale(1.12); }
  }
  @keyframes home-hero-drift-b {
    0%, 100% { transform: translate3d(0, 0, 0) scale(1); }
    50% { transform: translate3d(-34px, 30px, 0) scale(.88); }
  }
  @keyframes home-hero-orbit {
    from { transform: rotate(0deg) translateX(30px) rotate(0deg); }
    to { transform: rotate(360deg) translateX(30px) rotate(-360deg); }
  }
  @keyframes home-hero-bars {
    0%, 100% { transform: scaleY(.34); opacity: .35; }
    50% { transform: scaleY(1); opacity: 1; }
  }
  @keyframes home-hero-scan {
    from { transform: translateX(-145%); }
    to { transform: translateX(220%); }
  }
  @keyframes home-hero-pulse {
    0%, 100% { box-shadow: 0 0 0 0 rgba(63, 224, 208, .5); }
    50% { box-shadow: 0 0 0 8px rgba(63, 224, 208, 0); }
  }
  .home-hero {
    isolation: isolate;
    min-height: 335px;
  }
  .home-hero-copy {
    animation: home-hero-rise 760ms cubic-bezier(.16, 1, .3, 1) both;
  }
  .home-hero-detail {
    animation: home-hero-rise 820ms 130ms cubic-bezier(.16, 1, .3, 1) both;
  }
  .home-hero-orb-a { animation: home-hero-drift-a 12s ease-in-out infinite; }
  .home-hero-orb-b { animation: home-hero-drift-b 15s ease-in-out infinite; }
  .home-hero-orbit { animation: home-hero-orbit 13s linear infinite; }
  .home-hero-bar { transform-origin: bottom; animation: home-hero-bars 1.18s ease-in-out infinite; }
  .home-hero-scan { animation: home-hero-scan 5.5s linear infinite; }
  .home-hero-pulse { animation: home-hero-pulse 2s ease-out infinite; }
  @media (max-width: 720px) {
    .home-hero { min-height: 382px; }
    .home-hero-visual { opacity: .6; transform: translateX(22%); }
    .home-hero-title { font-size: 38px !important; max-width: 440px !important; }
  }
  @media (prefers-reduced-motion: reduce) {
    .home-hero-copy, .home-hero-detail, .home-hero-orb-a, .home-hero-orb-b, .home-hero-orbit, .home-hero-bar, .home-hero-scan, .home-hero-pulse { animation: none !important; }
  }
`;

function MotionField() {
  const bars = Array.from({ length: 22 }, (_, index) => ({
    height: 18 + ((index * 17) % 58),
    delay: `${(index % 7) * -0.16}s`,
  }));

  return (
    <div className="home-hero-visual" aria-hidden style={{ position: "absolute", inset: 0, overflow: "hidden", pointerEvents: "none" }}>
      <div
        style={{
          position: "absolute",
          inset: 0,
          opacity: 0.2,
          backgroundImage:
            "linear-gradient(rgba(255,255,255,.10) 1px, transparent 1px), linear-gradient(90deg, rgba(255,255,255,.10) 1px, transparent 1px)",
          backgroundSize: "32px 32px",
          maskImage: "linear-gradient(90deg, transparent 14%, black 55%, transparent)",
        }}
      />
      <div
        className="home-hero-orb-a"
        style={{
          position: "absolute",
          width: 390,
          height: 390,
          right: -110,
          top: -145,
          borderRadius: "50%",
          background: "radial-gradient(circle at 35% 35%, rgba(99,242,224,.68), rgba(45,153,185,.20) 35%, transparent 67%)",
          filter: "blur(2px)",
        }}
      />
      <div
        className="home-hero-orb-b"
        style={{
          position: "absolute",
          width: 330,
          height: 330,
          right: 95,
          bottom: -230,
          borderRadius: "50%",
          background: "radial-gradient(circle, rgba(109,119,255,.35), transparent 68%)",
          filter: "blur(8px)",
        }}
      />
      <div
        style={{
          position: "absolute",
          right: 78,
          top: 42,
          width: 192,
          height: 192,
          borderRadius: "50%",
          border: "1px solid rgba(183,255,246,.25)",
          boxShadow: "inset 0 0 34px rgba(73,218,207,.08), 0 0 80px rgba(31,182,168,.17)",
        }}
      >
        <span
          className="home-hero-orbit"
          style={{
            position: "absolute",
            top: "50%",
            left: "50%",
            width: 11,
            height: 11,
            margin: -5.5,
            borderRadius: "50%",
            background: palette.accent400,
            boxShadow: "0 0 18px 5px rgba(63,224,208,.44)",
          }}
        />
      </div>
      <div
        style={{
          position: "absolute",
          right: 30,
          bottom: 30,
          width: 272,
          height: 112,
          borderRadius: 16,
          border: "1px solid rgba(220,249,245,.15)",
          background: "linear-gradient(140deg, rgba(8,16,22,.34), rgba(38,88,97,.16))",
          backdropFilter: "blur(14px)",
          padding: "18px 18px 14px",
          overflow: "hidden",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", fontSize: 10, color: "rgba(230,251,248,.63)", letterSpacing: 1.2, textTransform: "uppercase" }}>
          <span>Voice signal</span>
          <span style={{ color: palette.accent400 }}>live</span>
        </div>
        <div style={{ height: 54, marginTop: 7, display: "flex", alignItems: "center", gap: 4 }}>
          {bars.map((bar, index) => (
            <span
              className="home-hero-bar"
              key={index}
              style={{
                display: "block",
                flex: 1,
                height: bar.height,
                borderRadius: 999,
                background: index % 5 === 0 ? "#95fff4" : "rgba(103,238,224,.74)",
                animationDelay: bar.delay,
              }}
            />
          ))}
        </div>
        <div className="home-hero-scan" style={{ position: "absolute", width: 80, height: "100%", top: 0, background: "linear-gradient(90deg, transparent, rgba(202,255,250,.20), transparent)", transform: "skewX(-18deg)" }} />
      </div>
      <div style={{ position: "absolute", inset: 0, background: "linear-gradient(90deg, rgba(13,19,25,.25), transparent 55%, rgba(13,19,25,.10))" }} />
    </div>
  );
}

function Banner({ today }: { today: number }) {
  return (
    <div
      className="home-hero"
      style={{
        position: "relative",
        overflow: "hidden",
        borderRadius: 22,
        padding: "34px 36px",
        background: "radial-gradient(circle at 84% 0%, #244d5d 0%, #162a35 28%, #10161c 65%)",
        boxShadow: "0 18px 44px rgba(10,18,24,.19), inset 0 1px 0 rgba(255,255,255,.12)",
      }}
    >
      <MotionField />
      <div
        style={{
          position: "absolute",
          inset: 0,
          opacity: 0.32,
          backgroundImage: "url(\"data:image/svg+xml,%3Csvg viewBox='0 0 180 180' xmlns='http://www.w3.org/2000/svg'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='.9' numOctaves='4' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)' opacity='.45'/%3E%3C/svg%3E\")",
          mixBlendMode: "soft-light",
          pointerEvents: "none",
        }}
      />
      <div className="home-hero-copy" style={{ position: "relative", maxWidth: 570 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, color: "rgba(218,243,234,.72)", fontSize: 10.5, fontWeight: 700, letterSpacing: 1.5, textTransform: "uppercase" }}>
          <span className="home-hero-pulse" style={{ width: 7, height: 7, borderRadius: "50%", background: palette.accent400 }} />
          Your voice workspace
        </div>
        <div
          className="home-hero-title"
          style={{
            fontFamily: font.serif,
            fontSize: 46,
            fontWeight: 600,
            letterSpacing: -1.6,
            color: palette.slate050,
            lineHeight: 0.99,
            maxWidth: 510,
            marginTop: 18,
          }}
        >
          Thought, in its clearest form.
        </div>
        <p style={{ color: "rgba(218,243,234,.74)", fontSize: 14, lineHeight: 1.6, margin: "16px 0 0", maxWidth: 450 }}>
          Speak naturally. WhimprFlow shapes the signal and delivers clean text exactly where you need it.
        </p>
      </div>
      <div className="home-hero-detail" style={{ position: "relative", display: "flex", alignItems: "center", gap: 12, marginTop: 26 }}>
        <div style={{ display: "inline-flex", alignItems: "center", gap: 8, padding: "8px 11px", border: "1px solid rgba(218,243,234,.16)", borderRadius: 10, background: "rgba(255,255,255,.07)", color: palette.slate050, fontSize: 12.5, fontWeight: 600, backdropFilter: "blur(8px)" }}>
          <Icon name="mic" size={15} strokeWidth={1.8} style={{ color: palette.accent400 }} />
          Ready to dictate
        </div>
        <span style={{ color: "rgba(218,243,234,.58)", fontSize: 12 }}>
          Hold <kbd style={{ fontFamily: font.mono, color: palette.slate050, fontSize: 10, padding: "3px 5px", margin: "0 3px", borderRadius: 4, border: "1px solid rgba(218,243,234,.22)", background: "rgba(255,255,255,.08)" }}>Fn</kbd> to begin
        </span>
        {today > 0 && <span style={{ color: palette.accent400, fontSize: 12, fontWeight: 600 }}>{fmtNum(today)} words today</span>}
      </div>
    </div>
  );
}

// ── History ──────────────────────────────────────────────────────────────────
type Group = { key: string; label: string; items: HistoryItem[] };

function groupByDay(items: HistoryItem[]): Group[] {
  const now = new Date();
  const groups: Group[] = [];
  const index = new Map<string, Group>();
  for (const it of items) {
    const d = new Date(it.ts_unix * 1000);
    const k = dayKey(d);
    let g = index.get(k);
    if (!g) {
      g = { key: k, label: dayLabel(d, now), items: [] };
      index.set(k, g);
      groups.push(g);
    }
    g.items.push(it);
  }
  return groups;
}

function HistoryRow({ item }: { item: HistoryItem }) {
  const d = new Date(item.ts_unix * 1000);
  return (
    <div style={{ display: "flex", gap: 14, padding: "11px 4px", borderBottom: `1px solid ${theme.border}` }}>
      <div
        style={{
          flex: "0 0 74px",
          fontSize: 12,
          color: theme.textFaint,
          fontVariantNumeric: "tabular-nums",
          paddingTop: 1,
        }}
      >
        {fmtTimeOfDay(d)}
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 13.5, lineHeight: 1.5, color: theme.textBody }}>{item.text}</div>
        {item.app && (
          <div style={{ fontSize: 11, color: theme.textFaint, marginTop: 3 }}>{item.app}</div>
        )}
      </div>
    </div>
  );
}

function HistorySection({ history }: { history: HistoryItem[] }) {
  const [query, setQuery] = useState("");
  const q = query.trim().toLowerCase();
  const filtered = q ? history.filter((h) => h.text.toLowerCase().includes(q)) : history;
  const groups = groupByDay(filtered);

  return (
    <Card pad={0}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 12,
          padding: "16px 18px",
          borderBottom: `1px solid ${theme.border}`,
        }}
      >
        <div
          style={{
            fontSize: 11.5,
            fontWeight: 700,
            letterSpacing: 0.7,
            textTransform: "uppercase",
            color: theme.textFaint,
          }}
        >
          Recent dictations
        </div>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 7,
            background: theme.cardBgSubtle,
            border: `1px solid ${theme.border}`,
            borderRadius: 9,
            padding: "6px 10px",
            minWidth: 180,
          }}
        >
          <Icon name="search" size={15} style={{ color: theme.textFaint }} />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search history"
            style={{
              border: "none",
              outline: "none",
              background: "transparent",
              fontFamily: font.ui,
              fontSize: 13,
              color: theme.textBody,
              width: "100%",
            }}
          />
        </div>
      </div>

      <div style={{ padding: "6px 18px 14px" }}>
        {history.length === 0 ? (
          <div style={{ padding: "36px 8px", textAlign: "center", color: theme.textFaint, fontSize: 13.5 }}>
            Your dictations will show up here. Hold your key and start speaking.
          </div>
        ) : filtered.length === 0 ? (
          <div style={{ padding: "36px 8px", textAlign: "center", color: theme.textFaint, fontSize: 13.5 }}>
            No dictations match “{query}”.
          </div>
        ) : (
          groups.map((g) => (
            <div key={g.key} style={{ marginTop: 14 }}>
              <div
                style={{
                  fontSize: 11,
                  fontWeight: 700,
                  letterSpacing: 0.6,
                  textTransform: "uppercase",
                  color: theme.accentDeep,
                  marginBottom: 2,
                }}
              >
                {g.label}
              </div>
              {g.items.map((it, i) => (
                <HistoryRow key={`${it.ts_unix}-${i}`} item={it} />
              ))}
            </div>
          ))
        )}
      </div>
    </Card>
  );
}

// ── Stats card (right column) ────────────────────────────────────────────────
function BigStat({ value, label, accent }: { value: string; label: string; accent?: boolean }) {
  return (
    <div style={{ flex: 1, textAlign: "center" }}>
      <div
        style={{
          fontFamily: font.serif,
          fontSize: 30,
          fontWeight: 600,
          lineHeight: 1.05,
          color: accent ? theme.accentDeep : theme.textStrong,
        }}
      >
        {value}
      </div>
      <div
        style={{
          fontSize: 10.5,
          color: theme.textFaint,
          marginTop: 6,
          textTransform: "uppercase",
          letterSpacing: 0.6,
        }}
      >
        {label}
      </div>
    </div>
  );
}

function StatsCard({ stats }: { stats: StatsSummary }) {
  const unlocked = stats.total_words >= UNLOCK_WORDS;
  return (
    <Card>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: 4 }}>
        <div style={{ fontSize: 14, fontWeight: 600, color: theme.textStrong }}>Your stats</div>
        <div style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12, color: theme.accentDeep, fontWeight: 600 }}>
          <Icon name="sparkles" size={14} strokeWidth={1.8} />
          keep it up
        </div>
      </div>

      <div style={{ textAlign: "center", margin: "16px 0 6px" }}>
        <div style={{ fontFamily: font.serif, fontSize: 42, fontWeight: 600, color: theme.textStrong, lineHeight: 1 }}>
          {fmtCompact(stats.total_words)}
        </div>
        <div style={{ fontSize: 11.5, color: theme.textFaint, marginTop: 6, textTransform: "uppercase", letterSpacing: 0.6 }}>
          total words
        </div>
      </div>

      <div style={{ fontSize: 12, color: theme.textMuted, textAlign: "center", marginBottom: 16 }}>
        {wordsReference(stats.total_words)}
      </div>

      <div
        style={{
          display: "flex",
          gap: 8,
          padding: "16px 0 0",
          borderTop: `1px solid ${theme.border}`,
        }}
      >
        <BigStat value={fmtNum(stats.avg_wpm)} label="avg WPM" accent />
        <BigStat value={`${stats.day_streak}`} label="day streak" />
      </div>

      {unlocked ? (
        <div style={{ fontSize: 12, color: theme.textFaint, textAlign: "center", marginTop: 14 }}>
          {fmtNum(stats.best_wpm)} WPM best · saved you {fmtDuration(stats.time_saved_secs)} vs typing
        </div>
      ) : (
        <div style={{ fontSize: 12, color: theme.textFaint, textAlign: "center", marginTop: 14, lineHeight: 1.5 }}>
          Keep dictating to unlock richer stats  -  {fmtNum(Math.max(0, UNLOCK_WORDS - stats.total_words))} words to go.
        </div>
      )}
    </Card>
  );
}

// ── Page ─────────────────────────────────────────────────────────────────────
export function Home() {
  const stats = useStats();
  const [history, setHistory] = useState<HistoryItem[]>([]);

  useEffect(() => {
    let alive = true;
    const load = () => getHistory().then((h) => alive && setHistory(h));
    load();
    const id = setInterval(load, 8000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);

  const today = stats.words_today;

  return (
    <div style={{ maxWidth: 1000 }}>
      <style>{HERO_MOTION_STYLES}</style>

      <Banner today={today} />

      <div style={{ display: "flex", alignItems: "baseline", justifyContent: "space-between", gap: 16, margin: "30px 0 18px" }}>
        <div>
          <div style={{ color: theme.accentDeep, fontSize: 10.5, fontWeight: 700, letterSpacing: 1.15, textTransform: "uppercase" }}>Your flow</div>
          <h1 style={{ fontFamily: font.serif, fontSize: 28, fontWeight: 600, letterSpacing: -0.6, margin: "5px 0 0", color: theme.textStrong }}>Welcome back</h1>
        </div>
        <p style={{ color: theme.textMuted, fontSize: 13, margin: 0, textAlign: "right" }}>
          {today > 0 ? `${fmtNum(today)} words dictated today.` : "Your workspace is ready."}
        </p>
      </div>

      <div style={{ display: "flex", flexWrap: "wrap", gap: 22, alignItems: "flex-start" }}>
        <div style={{ flex: "1 1 440px", minWidth: 0, display: "flex", flexDirection: "column", gap: 22 }}>
          <HistorySection history={history} />
        </div>
        <div style={{ flex: "0 0 300px", width: 300, maxWidth: "100%" }}>
          <StatsCard stats={stats} />
        </div>
      </div>
    </div>
  );
}
