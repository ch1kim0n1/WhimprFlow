# Linux: X11, XWayland, and Wayland

WhimprFlow's Linux platform layer uses **X11** (`x11rb` + `XGrabKey`) for the
global push-to-talk hotkey (Right Ctrl) and shells out to `xdotool` for paste and
foreground-window lookup.

## Wayland sessions

Native Wayland global shortcuts require compositor-specific portals
(`xdg-desktop-portal` global shortcuts / remote desktop). That path is not wired
yet.

| Session | Behavior |
| --- | --- |
| Pure X11 | Full hotkey + paste path |
| Wayland with XWayland | Hotkey grab may work for X11 clients only; Wayland-native apps may not receive synthetic paste |
| Pure Wayland, no XWayland | Hotkey install fails; the app still launches (Hub/tray) but **no global PTT**. Use the Hub UI to dictate where available, or switch to an X11 session |

## Runtime dependency

Install `xdotool` for paste/foreground detection:

```bash
sudo apt install xdotool   # Debian/Ubuntu
```

## Build dependencies (CI / from source)

```bash
sudo apt install build-essential pkg-config cmake libclang-dev \
  libdbus-1-dev libssl-dev libasound2-dev \
  libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev patchelf libx11-dev libxcb1-dev \
  libxkbcommon-dev libxrandr-dev
```

## Hub banner

When the X11 hotkey grab fails (typical on pure Wayland), the shell emits
`whimpr://linux/hotkeys-unavailable`. The Hub shows a banner explaining that
global push-to-talk is disabled; dictation from the Hub UI still works.
