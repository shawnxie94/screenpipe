// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

const mocks = vi.hoisted(() => ({
  configureWeReadKey: vi.fn(),
  getWeReadStatus: vi.fn(),
  removeWeReadKey: vi.fn(),
  saveWeReadScope: vi.fn(),
  startWeReadSync: vi.fn(),
}));

vi.mock("@/lib/connections/weread", () => mocks);

import { WeReadConnectionPanel } from "../weread-connection-panel";

const disconnected = {
  connector: "weread",
  key: "weread",
  auth_status: "disconnected",
  credential_configured: false,
  sync_status: "idle",
  scope: { auto_sync: false },
  scope_revision: 0,
  last_sync_at: null,
  last_success_at: null,
  imported_objects: 0,
  last_error_code: null,
  last_error_message: null,
};

const connected = {
  ...disconnected,
  auth_status: "authorized",
  credential_configured: true,
  scope_revision: 1,
};

describe("WeReadConnectionPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getWeReadStatus.mockResolvedValue(disconnected);
    mocks.configureWeReadKey.mockResolvedValue(undefined);
    mocks.removeWeReadKey.mockResolvedValue(undefined);
    mocks.saveWeReadScope.mockResolvedValue(2);
    mocks.startWeReadSync.mockResolvedValue(1);
  });

  it("accepts a key, clears the password field, and shows only credential state", async () => {
    mocks.getWeReadStatus
      .mockResolvedValueOnce(disconnected)
      .mockResolvedValueOnce(connected);
    const onChanged = vi.fn();
    render(<WeReadConnectionPanel onChanged={onChanged} />);

    const input = await screen.findByLabelText("微信读书 API Key");
    fireEvent.change(input, { target: { value: "wrk-secret-example" } });
    fireEvent.click(screen.getByRole("button", { name: "验证并连接" }));

    await waitFor(() => expect(mocks.configureWeReadKey).toHaveBeenCalledWith("wrk-secret-example"));
    expect(await screen.findByText("已连接")).toBeTruthy();
    expect(screen.queryByDisplayValue("wrk-secret-example")).toBeNull();
    expect(screen.queryByText("wrk-secret-example")).toBeNull();
    expect(onChanged).toHaveBeenCalled();
  });

  it("does not expose sync controls before a key is configured", async () => {
    render(<WeReadConnectionPanel />);
    await screen.findByLabelText("微信读书 API Key");
    expect(screen.queryByRole("button", { name: "立即同步" })).toBeNull();
    expect(screen.getByText(/不会下载书籍正文/)).toBeTruthy();
  });
});
