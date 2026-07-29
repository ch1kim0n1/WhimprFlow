import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LicensePane } from "./LicensePane";

const getEntitlement = vi.fn();
const activateLicense = vi.fn();
const startTrial = vi.fn();
const clearLicense = vi.fn();

vi.mock("./api", () => ({
  getEntitlement: (...a: unknown[]) => getEntitlement(...a),
  activateLicense: (...a: unknown[]) => activateLicense(...a),
  startTrial: (...a: unknown[]) => startTrial(...a),
  clearLicense: (...a: unknown[]) => clearLicense(...a),
}));

describe("LicensePane", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getEntitlement.mockResolvedValue({
      kind: "unlicensed",
      cloud_cleanup_allowed: false,
      email: null,
      tier: null,
      expires_unix: null,
      trial_days_remaining: null,
      purchase_url: "https://whimprflow.com/buy",
      message: "Enter a license key",
    });
  });

  it("activates a license key", async () => {
    activateLicense.mockResolvedValue({
      kind: "licensed",
      cloud_cleanup_allowed: true,
      email: "a@b.co",
      tier: "pro",
      expires_unix: null,
      trial_days_remaining: null,
      purchase_url: "https://whimprflow.com/buy",
      message: "License active",
    });
    render(<LicensePane />);
    await waitFor(() => expect(screen.getByText(/Unlicensed/i)).toBeInTheDocument());
    fireEvent.change(screen.getByPlaceholderText(/WF1/), { target: { value: "WF1.x.y" } });
    fireEvent.click(screen.getByRole("button", { name: /Activate/i }));
    await waitFor(() => expect(activateLicense).toHaveBeenCalledWith("WF1.x.y"));
    await waitFor(() => expect(screen.getByText(/Licensed/i)).toBeInTheDocument());
  });

  it("starts a trial", async () => {
    startTrial.mockResolvedValue({
      kind: "trial",
      cloud_cleanup_allowed: true,
      email: null,
      tier: "trial",
      expires_unix: 1,
      trial_days_remaining: 14,
      purchase_url: "https://whimprflow.com/buy",
      message: "Trial active",
    });
    render(<LicensePane />);
    await waitFor(() => screen.getByRole("button", { name: /Start 14-day trial/i }));
    fireEvent.click(screen.getByRole("button", { name: /Start 14-day trial/i }));
    await waitFor(() => expect(startTrial).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByText(/Status:\s*Trial/i)).toBeInTheDocument());
  });
});
