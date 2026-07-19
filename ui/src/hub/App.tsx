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
import { ScratchpadPane } from "./ScratchpadPane";
import { ShortcutsPane } from "./ShortcutsPane";
import { SettingsPane } from "./SettingsPane";
import { Help } from "./Help";
import { ComingSoon } from "./ComingSoon";
import { Walkthrough, shouldShowWalkthrough } from "./Walkthrough";
import { gsap, prefersReduced, EASE } from "./anim";
import {
  getSettings,
  setSettings,
  getStatus,
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

  const refresh = () => getStatus().then(setStatus);

  useEffect(() => {
    getSettings().then(setLocalSettings);
    refresh();
  }, []);

  const update = (s: Settings) => {
    setLocalSettings(s);
    void setSettings(s);
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
        <div style={{ padding: "36px 44px", margin: "0 auto", maxWidth: 1120 }}>
          <RoutedPage key={page} page={page}>
            {page === "home" && <Home />}
            {page === "insights" && <Insights />}
            {page === "dictionary" && <DictionaryPane />}
            {page === "snippets" && <SnippetsPane />}
            {page === "style" && <StylePane settings={settings} onChange={update} />}
            {page === "transforms" && <TransformsPane />}
            {page === "scratchpad" && <ScratchpadPane />}
            {page === "shortcuts" && <ShortcutsPane settings={settings} onChange={update} />}
            {page === "settings" && (
              <SettingsPane settings={settings} onChange={update} status={status} refresh={refresh} />
            )}
            {page === "help" && <Help />}
            {page === "account" && <ComingSoon icon="user" title="Account" desc="Account profiles and sync controls are on the way." />}
          </RoutedPage>
        </div>
      </main>
      {showWalkthrough && <Walkthrough setPage={setPage} onComplete={() => setShowWalkthrough(false)} />}
    </div>
  );
}
