import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(() => Promise.reject(new Error("no tauri"))),
}));

import {
  asrModelInstalled,
  DEFAULT_SETTINGS,
  EMPTY_STATS,
  getDictionary,
  getEntitlement,
  getHealth,
  getHistory,
  getSettings,
  getSnippets,
  getStats,
  getStatus,
  getWorkflows,
  listBackups,
  listModelOffers,
} from "./api";

describe("api browser fallbacks", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("getSettings returns DEFAULT_SETTINGS", async () => {
    await expect(getSettings()).resolves.toEqual(DEFAULT_SETTINGS);
  });

  it("getStatus returns all false", async () => {
    await expect(getStatus()).resolves.toEqual({
      accessibility: false,
      microphone: false,
      input_monitoring: false,
      has_openai_key: false,
      has_anthropic_key: false,
    });
  });

  it("getStats returns EMPTY_STATS", async () => {
    await expect(getStats()).resolves.toEqual(EMPTY_STATS);
  });

  it("getHistory returns []", async () => {
    await expect(getHistory()).resolves.toEqual([]);
  });

  it("getDictionary returns []", async () => {
    await expect(getDictionary()).resolves.toEqual([]);
  });

  it("getSnippets returns []", async () => {
    await expect(getSnippets()).resolves.toEqual([]);
  });

  it("getWorkflows returns []", async () => {
    await expect(getWorkflows()).resolves.toEqual([]);
  });

  it("getHealth returns not-ready", async () => {
    await expect(getHealth()).resolves.toEqual({
      asr_ready: false,
      asr_model: null,
      local_llm_ready: false,
      microphone: false,
      accessibility: false,
    });
  });

  it("listModelOffers returns []", async () => {
    await expect(listModelOffers()).resolves.toEqual([]);
  });

  it("asrModelInstalled returns false", async () => {
    await expect(asrModelInstalled()).resolves.toBe(false);
  });

  it("listBackups returns []", async () => {
    await expect(listBackups()).resolves.toEqual([]);
  });

  it("getEntitlement returns unlicensed", async () => {
    const e = await getEntitlement();
    expect(e.kind).toBe("unlicensed");
    expect(e.cloud_cleanup_allowed).toBe(false);
  });
});
