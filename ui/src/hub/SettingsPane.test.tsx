import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "../i18n";
import { SettingsPane } from "./SettingsPane";
import { DEFAULT_SETTINGS } from "./api";

const exportDiagnostics = vi.fn();
const backupData = vi.fn();
const listBackups = vi.fn();
const restoreBackup = vi.fn();
const checkForUpdate = vi.fn();
const setApiKey = vi.fn();
const requestMicrophone = vi.fn();
const requestAccessibility = vi.fn();
const requestInputMonitoring = vi.fn();

vi.mock("./api", async () => {
  const actual = await vi.importActual<typeof import("./api")>("./api");
  return {
    ...actual,
    exportDiagnostics: (...a: unknown[]) => exportDiagnostics(...a),
    backupData: (...a: unknown[]) => backupData(...a),
    listBackups: (...a: unknown[]) => listBackups(...a),
    restoreBackup: (...a: unknown[]) => restoreBackup(...a),
    checkForUpdate: (...a: unknown[]) => checkForUpdate(...a),
    setApiKey: (...a: unknown[]) => setApiKey(...a),
    requestMicrophone: (...a: unknown[]) => requestMicrophone(...a),
    requestAccessibility: (...a: unknown[]) => requestAccessibility(...a),
    requestInputMonitoring: (...a: unknown[]) => requestInputMonitoring(...a),
  };
});

const status = {
  accessibility: true,
  microphone: true,
  input_monitoring: true,
  has_openai_key: false,
  has_anthropic_key: false,
};

describe("SettingsPane", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listBackups.mockResolvedValue([]);
    checkForUpdate.mockResolvedValue(null);
  });

  it("renders Performance card values from settings", async () => {
    const settings = {
      ...DEFAULT_SETTINGS,
      max_ram_mb: 2048,
      unload_asr_after_idle_minutes: 5,
    };
    render(
      <SettingsPane settings={settings} onChange={() => {}} status={status} refresh={() => {}} />,
    );
    await waitFor(() => expect(screen.getByText(/Performance/i)).toBeInTheDocument());
    const ram = screen.getByDisplayValue("2048");
    const idle = screen.getByDisplayValue("5");
    expect(ram).toBeInTheDocument();
    expect(idle).toBeInTheDocument();
  });

  it("calls exportDiagnostics from the diagnostics control", async () => {
    exportDiagnostics.mockResolvedValue("/tmp/diag.zip");
    render(
      <SettingsPane
        settings={DEFAULT_SETTINGS}
        onChange={() => {}}
        status={status}
        refresh={() => {}}
      />,
    );
    const btn = await screen.findByRole("button", { name: /Export diagnostics/i });
    fireEvent.click(btn);
    await waitFor(() => expect(exportDiagnostics).toHaveBeenCalled());
  });

  it("runs backup / list / restore flow", async () => {
    backupData.mockResolvedValue("/tmp/backup");
    listBackups.mockResolvedValue(["/tmp/backup"]);
    restoreBackup.mockResolvedValue(3);
    render(
      <SettingsPane
        settings={DEFAULT_SETTINGS}
        onChange={() => {}}
        status={status}
        refresh={() => {}}
      />,
    );
    const backupBtn = await screen.findByRole("button", { name: /Back up now/i });
    fireEvent.click(backupBtn);
    await waitFor(() => expect(backupData).toHaveBeenCalled());
    await waitFor(() => expect(listBackups).toHaveBeenCalled());
  });
});
