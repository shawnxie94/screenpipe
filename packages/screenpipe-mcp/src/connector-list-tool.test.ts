// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

import { describe, expect, it } from "vitest";
import { handleListConnectors, sanitizeConnectorChannels } from "./connector-list-tool";

describe("connector list tool", () => {
  it("sanitizes channel responses to canonical IDs, names, and minimal status", () => {
    const connectors = sanitizeConnectorChannels({
      channels: [
        {
          connector: "office",
          entries: [
            {
              provider: "feishu",
              auth_status: "authorized",
              scope: { messages: "all" },
              access_token: "must-not-leak",
            },
          ],
        },
        {
          connector: "weread",
          entries: [
            {
              connector: "weread",
              auth_status: "disconnected",
              api_key: "secret-wrk-token",
              scope: { books: ["private"] },
            },
          ],
        },
        {
          connector: "rss",
          entries: [{ connector: "rss", error: { message: "private error" } }],
        },
      ],
    });

    expect(connectors).toEqual([
      { id: "office:feishu", name: "飞书", status: "connected" },
      { id: "rss", name: "RSS", status: "unknown" },
      { id: "weread", name: "微信读书", status: "disconnected" },
    ]);
    expect(JSON.stringify(connectors)).not.toContain("must-not-leak");
    expect(JSON.stringify(connectors)).not.toContain("secret-wrk-token");
    expect(JSON.stringify(connectors)).not.toContain("scope");
  });

  it("fetches the existing channels endpoint and returns only sanitized data", async () => {
    const calls: string[] = [];
    const fetchAPI = async (endpoint: string) => {
      calls.push(endpoint);
      return new Response(JSON.stringify({
        channels: [{ connector: "weread", entries: [{ connector: "weread", auth_status: "authorized", api_key: "secret" }] }],
      }), { status: 200 });
    };

    const result = await handleListConnectors(fetchAPI);

    expect(calls).toEqual(["/connections/channels"]);
    expect(result.structuredContent).toEqual({
      connectors: [{ id: "weread", name: "微信读书", status: "connected" }],
    });
    expect(result.content[0].text).not.toContain("secret");
  });

  it("fails closed when the endpoint is unavailable", async () => {
    await expect(
      handleListConnectors(async () => new Response("unavailable", { status: 503 })),
    ).rejects.toThrow("HTTP error: 503");
  });
});
