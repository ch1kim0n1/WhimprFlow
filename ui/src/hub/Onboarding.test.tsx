import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Onboarding } from "./Onboarding";

const asrModelInstalled = vi.fn();
const downloadAsrModel = vi.fn();
const requestAccessibility = vi.fn();
const requestMicrophone = vi.fn();
const requestInputMonitoring = vi.fn();
const micSelfTest = vi.fn();
const getEntitlement = vi.fn();
const startTrial = vi.fn();
const activateLicense = vi.fn();

vi.mock("./api", () => ({
  asrModelInstalled: (...a: unknown[]) => asrModelInstalled(...a),
  downloadAsrModel: (...a: unknown[]) => downloadAsrModel(...a),
  requestAccessibility: (...a: unknown[]) => requestAccessibility(...a),
  requestMicrophone: (...a: unknown[]) => requestMicrophone(...a),
  requestInputMonitoring: (...a: unknown[]) => requestInputMonitoring(...a),
  micSelfTest: (...a: unknown[]) => micSelfTest(...a),
  getEntitlement: (...a: unknown[]) => getEntitlement(...a),
  startTrial: (...a: unknown[]) => startTrial(...a),
  activateLicense: (...a: unknown[]) => activateLicense(...a),
}));

vi.mock("./platform", () => ({
  detectPlatformSync: () => "macos",
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

const baseStatus = {
  accessibility: false,
  microphone: false,
  input_monitoring: false,
  has_openai_key: false,
  has_anthropic_key: false,
};

describe("Onboarding", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    asrModelInstalled.mockResolvedValue(false);
    getEntitlement.mockResolvedValue({
      kind: "unlicensed",
      cloud_cleanup_allowed: false,
      email: null,
      tier: null,
      expires_unix: null,
      trial_days_remaining: null,
      purchase_url: "https://whimprflow.com/buy",
      message: "",
    });
  });

  it("blocks enter without accessibility", async () => {
    render(
      <Onboarding
        status={{ ...baseStatus, accessibility: false, microphone: true }}
        refresh={() => {}}
        onEnter={() => {}}
      />,
    );
    await waitFor(() => expect(screen.getByRole("button", { name: /Finish permissions/i })).toBeDisabled());
  });

  it("blocks enter without microphone", async () => {
    render(
      <Onboarding
        status={{ ...baseStatus, accessibility: true, microphone: false }}
        refresh={() => {}}
        onEnter={() => {}}
      />,
    );
    await waitFor(() => expect(screen.getByRole("button", { name: /Finish permissions/i })).toBeDisabled());
  });

  it("can enter after permissions, mic skip, and model", async () => {
    asrModelInstalled.mockResolvedValue(true);
    const onEnter = vi.fn();
    render(
      <Onboarding
        status={{ ...baseStatus, accessibility: true, microphone: true }}
        refresh={() => {}}
        onEnter={onEnter}
      />,
    );
    await waitFor(() => screen.getByRole("button", { name: /^Skip$/i }));
    fireEvent.click(screen.getByRole("button", { name: /^Skip$/i }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /Enter WhimprFlow/i })).not.toBeDisabled(),
    );
  });

  it("downloads the base.en model", async () => {
    downloadAsrModel.mockResolvedValue("ok");
    render(
      <Onboarding
        status={{ ...baseStatus, accessibility: true, microphone: true }}
        refresh={() => {}}
        onEnter={() => {}}
      />,
    );
    await waitFor(() => screen.getByRole("button", { name: /^Skip$/i }));
    fireEvent.click(screen.getByRole("button", { name: /^Skip$/i }));
    await waitFor(() => screen.getByRole("button", { name: /^Download$/i }));
    fireEvent.click(screen.getByRole("button", { name: /^Download$/i }));
    await waitFor(() => expect(downloadAsrModel).toHaveBeenCalledWith("base.en"));
  });

  it("supports skipping the mic self-test", async () => {
    asrModelInstalled.mockResolvedValue(true);
    render(
      <Onboarding
        status={{ ...baseStatus, accessibility: true, microphone: true }}
        refresh={() => {}}
        onEnter={() => {}}
      />,
    );
    await waitFor(() => screen.getByRole("button", { name: /^Skip$/i }));
    fireEvent.click(screen.getByRole("button", { name: /^Skip$/i }));
    expect(micSelfTest).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByText(/Mic test skipped/i)).toBeInTheDocument());
  });
});
