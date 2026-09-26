// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

export type ConnectorListItem = {
  id: string;
  name: string;
  status: "connected" | "disconnected" | "unknown";
};

type ConnectorChannelEntry = {
  connector?: unknown;
  provider?: unknown;
  key?: unknown;
  auth_status?: unknown;
  connected?: unknown;
};

type FetchApi = (endpoint: string, options?: RequestInit) => Promise<Response>;

const CONNECTOR_NAMES: Record<string, string> = {
  "office:feishu": "飞书",
  "office:tencent-meeting": "腾讯会议",
  rss: "RSS",
  weread: "微信读书",
};

function toConnectorListItem(
  channelId: string,
  entry: ConnectorChannelEntry,
): ConnectorListItem | undefined {
  const provider = typeof entry.provider === "string"
    ? entry.provider
    : typeof entry.key === "string" && entry.key
      ? entry.key
      : undefined;
  const id = typeof entry.connector === "string" && entry.connector !== "office"
    ? entry.connector
    : channelId === "office" && provider
      ? `office:${provider}`
      : channelId;
  if (!id) return undefined;

  const authStatus = typeof entry.auth_status === "string"
    ? entry.auth_status.toLowerCase()
    : "";
  const status = entry.connected === true || authStatus === "authorized"
    ? "connected"
    : entry.connected === false || authStatus === "disconnected"
      ? "disconnected"
      : "unknown";
  return { id, name: CONNECTOR_NAMES[id] ?? id, status };
}

export function sanitizeConnectorChannels(payload: unknown): ConnectorListItem[] {
  const channels = (payload as { channels?: unknown } | null)?.channels;
  if (!Array.isArray(channels)) return [];
  const byId = new Map<string, ConnectorListItem>();

  for (const rawChannel of channels) {
    if (!rawChannel || typeof rawChannel !== "object") continue;
    const channel = rawChannel as { connector?: unknown; entries?: unknown };
    if (typeof channel.connector !== "string") continue;
    const entries = Array.isArray(channel.entries) ? channel.entries : [];
    for (const rawEntry of entries) {
      if (!rawEntry || typeof rawEntry !== "object") continue;
      const item = toConnectorListItem(channel.connector, rawEntry as ConnectorChannelEntry);
      if (item) byId.set(item.id, item);
    }
  }
  return [...byId.values()].sort((a, b) => a.id.localeCompare(b.id));
}

export async function handleListConnectors(fetchAPI: FetchApi) {
  const response = await fetchAPI("/connections/channels");
  if (!response.ok) {
    throw new Error(`HTTP error: ${response.status} while listing connectors`);
  }
  const connectors = sanitizeConnectorChannels(await response.json());
  return {
    content: [{ type: "text" as const, text: JSON.stringify({ connectors }) }],
    structuredContent: { connectors },
  };
}
