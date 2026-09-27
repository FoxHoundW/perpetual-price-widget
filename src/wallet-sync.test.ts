import { beforeEach, describe, expect, it, vi } from "vitest";
import { watchWallet } from "./wallet-sync";

const mocks = vi.hoisted(() => ({ listen: vi.fn(), invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

describe("shared wallet updates", () => {
  beforeEach(() => vi.resetAllMocks());
  it("reads once and then consumes every pushed result without additional requests", async () => {
    const stop = vi.fn();
    let deliver!: (event: { payload: unknown }) => void;
    mocks.listen.mockImplementation(async (_, callback) => { deliver = callback; return stop; });
    mocks.invoke.mockResolvedValue({ balance: "1.00" });
    const render = vi.fn();
    const cleanup = await watchWallet(render);
    deliver({ payload: { balance: "2.00" } });
    deliver({ payload: { balance: null, error: "断线" } });
    deliver({ payload: { balance: "3.00" } });
    expect(render.mock.calls.map(call => call[0].balance)).toEqual(["1.00", "2.00", null, "3.00"]);
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
    cleanup();
    expect(stop).toHaveBeenCalledOnce();
  });
  it("does not replace a fresh event with an older initial cache read", async () => {
    let deliver!: (event: { payload: unknown }) => void;
    mocks.listen.mockImplementation(async (_, callback) => { deliver = callback; return vi.fn(); });
    mocks.invoke.mockImplementation(async () => {
      deliver({ payload: { balance: "2.00" } });
      return { balance: "1.00" };
    });
    const render = vi.fn();
    await watchWallet(render);
    expect(render).toHaveBeenCalledExactlyOnceWith({ balance: "2.00" });
  });
  it("recovers via events when the initial cache read fails", async () => {
    let deliver!: (event: { payload: unknown }) => void;
    mocks.listen.mockImplementation(async (_, callback) => { deliver = callback; return vi.fn(); });
    mocks.invoke.mockRejectedValue(new Error("unavailable"));
    const render = vi.fn();
    await watchWallet(render);
    deliver({ payload: { balance: "4.00" } });
    expect(render).toHaveBeenCalledExactlyOnceWith({ balance: "4.00" });
  });
});
