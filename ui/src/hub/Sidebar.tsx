import { useLayoutEffect, useRef } from "react";
import { font } from "../tokens/values";
import { theme } from "./theme";
import { Icon, type IconName } from "./icons";
import { gsap, prefersReduced, EASE } from "./anim";
import brandIcon from "../assets/whimprflow-icon.png";

export type Page =
  | "home"
  | "insights"
  | "dictionary"
  | "snippets"
  | "style"
  | "transforms"
  | "scratchpad"
  | "shortcuts"
  | "settings"
  | "help"
  | "account";

type NavDef = { key: Page; label: string; icon: IconName };

const MAIN: NavDef[] = [
  { key: "home", label: "Home", icon: "home" },
  { key: "insights", label: "Insights", icon: "insights" },
  { key: "dictionary", label: "Dictionary", icon: "dictionary" },
  { key: "snippets", label: "Snippets", icon: "snippets" },
  { key: "style", label: "Style", icon: "style" },
  { key: "transforms", label: "Transforms", icon: "transforms" },
  { key: "scratchpad", label: "Scratchpad", icon: "scratchpad" },
];

const BOTTOM: NavDef[] = [
  { key: "shortcuts", label: "Shortcuts", icon: "shortcuts" },
  { key: "settings", label: "Settings", icon: "settings" },
  { key: "help", label: "Help", icon: "help" },
];

const ACCOUNT: NavDef = { key: "account", label: "Account", icon: "user" };

const NAV_CSS = `
  .nav-item { position: relative; z-index: 1; }
  .nav-item:hover:not(.nav-active) { background: ${theme.hover}; }
  .nav-item .nav-ico { transition: transform 200ms cubic-bezier(.16,1,.3,1); }
  .nav-item:hover .nav-ico { transform: translateX(2px); }
`;

function NavItem({ item, active, onClick, collapsed }: { item: NavDef; active: boolean; onClick: () => void; collapsed: boolean }) {
  const isAccount = item.key === "account";
  return (
    <button
      data-page={item.key}
      onClick={onClick}
      className={`nav-item${active ? " nav-active" : ""}`}
      aria-label={item.label}
      title={collapsed ? item.label : undefined}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 11,
        width: "100%",
        textAlign: "left",
        border: "none",
        cursor: "pointer",
        padding: collapsed ? "10px" : "9px 11px",
        borderRadius: 10,
        fontSize: 13.5,
        fontFamily: font.ui,
        fontWeight: active ? 600 : 500,
        color: active ? theme.accentDeep : theme.textBody,
        background: "transparent",
        justifyContent: collapsed ? "center" : "flex-start",
        transition: "color 180ms ease, background 140ms ease, padding 180ms ease",
      }}
    >
      <span
        className="nav-ico"
        style={{
          display: "inline-flex",
          alignItems: "center",
          justifyContent: "center",
          width: isAccount ? 22 : undefined,
          height: isAccount ? 22 : undefined,
          borderRadius: isAccount ? "50%" : undefined,
          background: isAccount ? theme.accentSoft : undefined,
          border: isAccount ? `1px solid ${theme.accentSoftBorder}` : undefined,
        }}
      >
        <Icon name={item.icon} size={18} style={{ color: active ? theme.accentDeep : theme.textMuted }} />
      </span>
      {!collapsed && item.label}
    </button>
  );
}

export function Sidebar({
  page,
  setPage,
  collapsed,
  onCollapsedChange,
}: {
  page: Page;
  setPage: (p: Page) => void;
  collapsed: boolean;
  onCollapsedChange: (collapsed: boolean) => void;
}) {
  const asideRef = useRef<HTMLElement | null>(null);
  const pillRef = useRef<HTMLDivElement | null>(null);
  const mounted = useRef(false);

  useLayoutEffect(() => {
    const aside = asideRef.current;
    const pill = pillRef.current;
    if (!aside || !pill) return;

    const place = (animate: boolean) => {
      const btn = aside.querySelector<HTMLElement>(`[data-page="${page}"]`);
      if (!btn) return;
      const a = aside.getBoundingClientRect();
      const b = btn.getBoundingClientRect();
      const to = { x: b.left - a.left, y: b.top - a.top, width: b.width, height: b.height, opacity: 1 };
      if (animate && !prefersReduced() && !document.hidden) {
        gsap.to(pill, { ...to, duration: 0.42, ease: EASE });
      } else {
        gsap.set(pill, to);
      }
    };

    place(mounted.current);
    if (!mounted.current && !prefersReduced() && !document.hidden) {
      gsap.fromTo(pill, { opacity: 0, scale: 0.9 }, { opacity: 1, scale: 1, duration: 0.5, ease: EASE, transformOrigin: "left center" });
    }
    mounted.current = true;

    const onResize = () => place(false);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [page]);

  return (
    <aside
      ref={asideRef}
      style={{
        position: "relative",
        width: collapsed ? 74 : 230,
        flex: `0 0 ${collapsed ? 74 : 230}px`,
        borderRight: `1px solid ${theme.border}`,
        background: theme.sidebarBg,
        backgroundImage: theme.sidebarGradient,
        display: "flex",
        flexDirection: "column",
        padding: collapsed ? "20px 10px 16px" : "20px 14px 16px",
        transition: "width 220ms cubic-bezier(.16,1,.3,1), flex-basis 220ms cubic-bezier(.16,1,.3,1), padding 220ms cubic-bezier(.16,1,.3,1)",
      }}
    >
      <style>{NAV_CSS}</style>

      {/* Sliding active indicator (single pill that glides between items) */}
      <div
        ref={pillRef}
        aria-hidden
        style={{
          position: "absolute",
          left: 0,
          top: 0,
          zIndex: 0,
          opacity: 0,
          borderRadius: 10,
          background: theme.accentSoft,
          border: `1px solid ${theme.accentSoftBorder}`,
          boxShadow: "0 1px 2px rgba(17,20,25,.04)",
          pointerEvents: "none",
        }}
      />

      {/* Wordmark, local marker, and compact-mode control. */}
      <div style={{ display: "flex", alignItems: "center", justifyContent: collapsed ? "center" : "space-between", gap: 9, padding: collapsed ? "0 0 20px" : "0 8px 20px", position: "relative", zIndex: 1 }}>
        <div style={{ display: "flex", minWidth: 0, alignItems: "center", gap: 9 }}>
        <img
          src={brandIcon}
          alt=""
          width={26}
          height={26}
          style={{ borderRadius: 7, display: "block", boxShadow: theme.shadowSoft }}
        />
        {!collapsed && <span style={{ fontFamily: font.serif, fontSize: 19, fontWeight: 600, letterSpacing: -0.3, color: theme.textStrong }}>
          WhimprFlow
        </span>}
        {!collapsed && <span
          style={{
            fontSize: 10, fontWeight: 700, letterSpacing: 0.4, textTransform: "uppercase",
            color: theme.accentDeep, background: theme.accentSoft, border: `1px solid ${theme.accentSoftBorder}`,
            borderRadius: 999, padding: "2px 7px",
          }}
        >
          Local
        </span>}
        </div>
        {!collapsed && <button
          onClick={() => onCollapsedChange(true)}
          aria-label="Collapse sidebar"
          title="Collapse sidebar"
          style={{ width: 26, height: 26, padding: 0, borderRadius: 8, display: "grid", placeItems: "center", cursor: "pointer", color: theme.textMuted, background: "transparent", border: `1px solid ${theme.border}` }}
        >
          <Icon name="panelLeft" size={15} strokeWidth={1.8} />
        </button>}
      </div>

      {collapsed && <button
        onClick={() => onCollapsedChange(false)}
        aria-label="Expand sidebar"
        title="Expand sidebar"
        style={{ width: "100%", height: 30, marginBottom: 10, padding: 0, borderRadius: 9, display: "grid", placeItems: "center", cursor: "pointer", color: theme.textMuted, background: "rgba(255,255,255,0.20)", border: `1px solid ${theme.border}` }}
      >
        <Icon name="panelRight" size={15} strokeWidth={1.8} />
      </button>}

      <nav style={{ display: "flex", flexDirection: "column", gap: 3, position: "relative", zIndex: 1 }}>
        {MAIN.map((n) => (
          <NavItem key={n.key} item={n} active={page === n.key} onClick={() => setPage(n.key)} collapsed={collapsed} />
        ))}
      </nav>

      <div style={{ flex: 1 }} />

      <nav style={{ display: "flex", flexDirection: "column", gap: 3, paddingTop: 12, borderTop: `1px solid ${theme.border}`, position: "relative", zIndex: 1 }}>
        {BOTTOM.map((n) => (
          <NavItem key={n.key} item={n} active={page === n.key} onClick={() => setPage(n.key)} collapsed={collapsed} />
        ))}
      </nav>
      <div style={{ paddingTop: 12, marginTop: 12, borderTop: `1px solid ${theme.border}`, position: "relative", zIndex: 1 }}>
        <NavItem item={ACCOUNT} active={page === ACCOUNT.key} onClick={() => setPage(ACCOUNT.key)} collapsed={collapsed} />
      </div>
    </aside>
  );
}
