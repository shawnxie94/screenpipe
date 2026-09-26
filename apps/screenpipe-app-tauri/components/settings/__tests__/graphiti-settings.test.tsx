// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { GraphitiSettings } from "../graphiti-settings";

const { appServerFetch } = vi.hoisted(() => ({ appServerFetch: vi.fn() }));
vi.mock("@/lib/notifications/app-server", () => ({ appServerFetch }));

const initialSettings = {
  adapter_url: null,
  allow_http_localhost: false,
  auto_sync_enabled: false,
  search_enabled: false,
  sync_interval_seconds: 30,
};

function jsonResponse(value: unknown, status = 200) {
  return new Response(JSON.stringify(value), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

beforeEach(() => {
  appServerFetch.mockReset();
  appServerFetch.mockResolvedValue(jsonResponse(initialSettings));
});

describe("GraphitiSettings", () => {
  it("loads and persists independent default-off settings", async () => {
    appServerFetch.mockImplementation(async (_path: string, init?: RequestInit) =>
      init?.method === "PUT"
        ? jsonResponse(JSON.parse(String(init.body)))
        : jsonResponse(initialSettings),
    );
    render(<GraphitiSettings />);

    const searchToggle = await screen.findByRole("checkbox", { name: "启用 Graphiti 搜索" });
    const syncToggle = screen.getByRole("checkbox", { name: "启用 Graphiti 自动同步" });
    expect(searchToggle).not.toBeChecked();
    expect(syncToggle).not.toBeChecked();

    fireEvent.click(searchToggle);
    fireEvent.change(screen.getByLabelText("Graphiti 适配器地址"), {
      target: { value: "https://graphiti.example.ts.net" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存设置" }));

    await waitFor(() => expect(appServerFetch).toHaveBeenCalledWith(
      "/graphiti/settings",
      expect.objectContaining({ method: "PUT" }),
    ));
    const request = appServerFetch.mock.calls.find(([, init]) => init?.method === "PUT");
    expect(JSON.parse(String(request?.[1]?.body))).toMatchObject({
      adapter_url: "https://graphiti.example.ts.net",
      auto_sync_enabled: false,
      search_enabled: true,
      sync_interval_seconds: 30,
    });
    expect(await screen.findByRole("status")).toHaveTextContent("已保存并立即生效");
  });

  it("rejects an unsafe adapter URL before sending a mutation", async () => {
    render(<GraphitiSettings />);
    const input = await screen.findByLabelText("Graphiti 适配器地址");
    fireEvent.change(input, { target: { value: "https://user:secret@example.com/?token=x" } });
    fireEvent.click(screen.getByRole("button", { name: "保存设置" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("不能包含账号、密码、路径、查询参数或片段");
    expect(appServerFetch).toHaveBeenCalledTimes(1);
  });
});
