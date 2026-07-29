import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { UpdatesCard } from "./SettingsPane";

const checkForUpdate = vi.fn();
const relaunchAfterUpdate = vi.fn();

vi.mock("./api", () => ({
  checkForUpdate: (...a: unknown[]) => checkForUpdate(...a),
  relaunchAfterUpdate: (...a: unknown[]) => relaunchAfterUpdate(...a),
}));

describe("UpdatesCard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows up to date when check returns null", async () => {
    checkForUpdate.mockResolvedValue(null);
    render(<UpdatesCard />);
    fireEvent.click(screen.getByRole("button", { name: /Check for updates/i }));
    await waitFor(() => expect(screen.getByText(/latest version/i)).toBeInTheDocument());
  });

  it("shows version and download when an update is available", async () => {
    const downloadAndInstall = vi.fn().mockResolvedValue(undefined);
    checkForUpdate.mockResolvedValue({
      version: "1.2.3",
      body: "notes",
      downloadAndInstall,
    });
    render(<UpdatesCard />);
    fireEvent.click(screen.getByRole("button", { name: /Check for updates/i }));
    await waitFor(() => screen.getByRole("button", { name: /Install version 1.2.3/i }));
    fireEvent.click(screen.getByRole("button", { name: /Install version 1.2.3/i }));
    await waitFor(() => expect(downloadAndInstall).toHaveBeenCalled());
    await waitFor(() => expect(relaunchAfterUpdate).toHaveBeenCalled());
  });

  it("shows error state when download fails", async () => {
    checkForUpdate.mockResolvedValue({
      version: "9.9.9",
      downloadAndInstall: vi.fn().mockRejectedValue(new Error("fail")),
    });
    render(<UpdatesCard />);
    fireEvent.click(screen.getByRole("button", { name: /Check for updates/i }));
    await waitFor(() => screen.getByRole("button", { name: /Install version 9.9.9/i }));
    fireEvent.click(screen.getByRole("button", { name: /Install version 9.9.9/i }));
    await waitFor(() => expect(screen.getByText(/Update failed/i)).toBeInTheDocument());
  });
});
