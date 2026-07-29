import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Onboarding } from "./Onboarding";
import { Sidebar, type Page } from "./Sidebar";
import { Home } from "./Home";
import { Insights } from "./Insights";
import { DictionaryPane } from "./DictionaryPane";
import { SnippetsPane } from "./SnippetsPane";
import { StylePane } from "./StylePane";
import { TransformsPane } from "./TransformsPane";
import { WorkflowsPane } from "./WorkflowsPane";
import { ScratchpadPane } from "./ScratchpadPane";
import { MemoryPane } from "./MemoryPane";
import { PrivacyPane } from "./PrivacyPane";
import { ShortcutsPane } from "./ShortcutsPane";
import { SettingsPane } from "./SettingsPane";
import { Help } from "./Help";
import { LicensePane } from "./LicensePane";
import { Walkthrough, shouldShowWalkthrough } from "./Walkthrough";
import { gsap, prefersReduced, EASE } from "./anim";
import {
  dismissSafeMode,
  getSettings,
  setSettings,
  getStatus,
  hubReady,
  onCloudUnavailable,
  onSafeMode,
  type Settings,
  type Status,
  DEFAULT_SETTINGS,
} from "./api";

// Wraps the routed pane. Remounted per navigation (key={page}), so each page
// arrival plays a GSAP enter-cascade: the pane's own sections stagger up. Home
// runs its own richer timeline, so it opts out here.
function RoutedPage({ page, children }: { page: Page; children: React.ReactNode }) {
  const ref = useRef<HTMLDivElement | null>(null);
  useLayoutEffect(() => {
    if (page === "home" || prefersReduced() || document.hidden || !ref.current) return;
    const ctx = gsap.context(() => {
      const root = ref.current?.firstElementChild;
      const targets = root && root.children.length > 1 ? root.children : ref.current?.children;
      gsap.from(targets as Element[] | HTMLCollection, {
        opacity: 0,
        y: 22,
        duration: 0.6,
        ease: EASE,
        stagger: 0.07,
        clearProps: "transform,opacity",
      });
    }, ref);
    return () => ctx.revert();
  }, [page]);
  return <div ref={ref}>{children}</div>;
}

export function App() {
  const [page, setPage] = useState<Page>("home");
  const [sidebarCollapsed, setSidebarCollapsed] = useState(() => {
    try {
      return localStorage.getItem("whimpr:sidebar-collapsed") === "true";
    } catch {
      return false;
    }
  });
  const [showWalkthrough, setShowWalkthrough] = useState(shouldShowWalkthrough);
  const [settings, setLocalSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [entered, setEntered] = useState(false);
  const [status, setStatus] = useState<Status>({
    accessibility: false,
    microphone: false,
    input_monitoring: false,
    has_openai_key: false,
    has_anthropic_key: false,
  });
  const [cloudBanner, setCloudBanner] = useState<string | null>(null);
  const [safeMode, setSafeMode] = useState(false);

  const refresh = () => getStatus().then(setStatus);

  useEffect(() => {
    getSettings().then(setLocalSettings);
    refresh();
    void hubReady();
  }, []);

  useEffect(() => {
    let un: (() => void) | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    void onCloudUnavailable((msg) => {
      setCloudBanner(
        msg?.trim()
          ? msg
          : "Cloud cleanup was unavailable — your last dictation was pasted raw. Check your API key and network.",
      );
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => setCloudBanner(null), 10_000);
    }).then((fn) => {
      un = fn;
    });
    return () => {
      un?.();
      if (timer) clearTimeout(timer);
    };
  }, []);

  useEffect(() => {
    let un: (() => void) | undefined;
    void onSafeMode(() => setSafeMode(true)).then((fn) => {
      un = fn;
    });
    return () => un?.();
  }, []);

  const update = (s: Settings) => {
    setLocalSettings(s);
    void setSettings(s).then((next) => {
      if (next) setLocalSettings(next);
    });
  };

  const setCollapsed = (collapsed: boolean) => {
    setSidebarCollapsed(collapsed);
    try {
      localStorage.setItem("whimpr:sidebar-collapsed", String(collapsed));
    } catch {
      // The state remains usable when browser storage is unavailable.
    }
  };

  // Gate the app behind the setup wizard until the required permissions are granted.
  if (!(status.accessibility && status.microphone) && !entered) {
    return <Onboarding status={status} refresh={refresh} onEnter={() => setEntered(true)} />;
  }

  return (
    <div
      style={{
        display: "flex",
        height: "100vh",
        fontFamily: font.ui,
        color: theme.textBody,
        background: theme.pageBg,
      }}
    >
      <Sidebar page={page} setPage={setPage} collapsed={sidebarCollapsed} onCollapsedChange={setCollapsed} />
      <main style={{ flex: 1, minWidth: 0, overflowY: "auto" }}>
        {cloudBanner && (
          <div
            role="status"
            onClick={() => setCloudBanner(null)}
            style={{
              margin: "12px 44px 0",
              padding: "12px 16px",
              borderRadius: 12,
              background: theme.accentSoft,
              border: `1px solid ${theme.accentSoftBorder}`,
              color: theme.textStrong,
              fontSize: 13.5,
              lineHeight: 1.45,
              cursor: "pointer",
            }}
          >
            {cloudBanner.includes("Cloud cleanup")
              ? cloudBanner
              : "Cloud cleanup was unavailable — your last dictation was pasted raw. Check your API key and network."}
          </div>
        )}
        <div style={{ padding: "36px 44px", margin: "0 auto", maxWidth: 1120 }}>
          <RoutedPage key={page} page={page}>
            {page === "home" && <Home />}
            {page === "insights" && <Insights />}
            {page === "dictionary" && <DictionaryPane />}
            {page === "snippets" && <SnippetsPane />}
            {page === "style" && <StylePane settings={settings} onChange={update} />}
            {page === "transforms" && <TransformsPane />}
            {page === "workflows" && <WorkflowsPane />}
            {page === "scratchpad" && <ScratchpadPane />}
            {page === "memory" && <MemoryPane />}
            {page === "privacy" && <PrivacyPane settings={settings} onChange={update} status={status} />}
            {page === "shortcuts" && <ShortcutsPane settings={settings} onChange={update} />}
            {page === "settings" && (
              <SettingsPane settings={settings} onChange={update} status={status} refresh={refresh} />
            )}
            {page === "help" && <Help />}
            {page === "account" && <LicensePane />}
          </RoutedPage>
        </div>
      </main>
      {showWalkthrough && <Walkthrough setPage={setPage} onComplete={() => setShowWalkthrough(false)} />}
      {safeMode && (
        <div
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(20, 18, 16, 0.45)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 1000,
            padding: 24,
          }}
        >
          <div
            style={{
              width: 440,
              maxWidth: "100%",
              background: theme.cardBg,
              border: `1px solid ${theme.border}`,
              borderRadius: 16,
              boxShadow: theme.shadow,
              padding: 24,
            }}
          >
            <div style={{ fontSize: 17, fontWeight: 700, color: theme.textStrong, marginBottom: 8 }}>
              Safe mode
            </div>
            <p style={{ margin: "0 0 18px", fontSize: 14, color: theme.textMuted, lineHeight: 1.5 }}>
              WhimprFlow failed to start 3 times. You may need to reinstall the previous version.
            </p>
            <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
              <a
                href="https://github.com/ch1kim0n1/WhimprFlow/releases"
                target="_blank"
                rel="noreferrer"
                style={{
                  textAlign: "center",
                  textDecoration: "none",
                  borderRadius: 10,
                  padding: "11px 14px",
                  fontSize: 13.5,
                  fontWeight: 650,
                  color: theme.solidText,
                  background: theme.solidBg,
                }}
              >
                Download previous version
              </a>
              <button
                type="button"
                onClick={() => {
                  void dismissSafeMode().then(() => setSafeMode(false));
                }}
                style={{
                  cursor: "pointer",
                  border: `1px solid ${theme.border}`,
                  borderRadius: 10,
                  padding: "11px 14px",
                  fontSize: 13.5,
                  fontWeight: 600,
                  fontFamily: font.ui,
                  color: theme.textStrong,
                  background: theme.cardBgSubtle,
                }}
              >
                Continue anyway (may crash again)
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
