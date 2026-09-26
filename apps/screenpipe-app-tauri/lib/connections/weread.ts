// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

import { localFetch } from "@/lib/api";

const BASE = "/connections/weread";

export interface WeReadStatus {
  connector: string;
  key: string;
  auth_status: "authorized" | "disconnected" | string;
  credential_configured: boolean;
  sync_status: string;
  scope: { auto_sync: boolean };
  scope_revision: number;
  last_sync_at: string | null;
  last_success_at: string | null;
  imported_objects: number;
  last_error_code: string | null;
  last_error_message: string | null;
}

async function responseError(response: Response, fallback: string): Promise<Error> {
  const body = (await response.json().catch(() => ({}))) as { message?: string };
  return new Error(body.message || `${fallback} (${response.status})`);
}

export async function getWeReadStatus(): Promise<WeReadStatus> {
  const response = await localFetch(BASE);
  if (!response.ok) throw await responseError(response, "加载微信读书连接失败");
  return (await response.json()) as WeReadStatus;
}

export async function configureWeReadKey(apiKey: string): Promise<void> {
  const response = await localFetch(`${BASE}/key`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ api_key: apiKey }),
  });
  if (!response.ok) throw await responseError(response, "验证或保存 API Key 失败");
}

export async function removeWeReadKey(): Promise<void> {
  const response = await localFetch(`${BASE}/key`, { method: "DELETE" });
  if (!response.ok) throw await responseError(response, "移除微信读书连接失败");
}

export async function saveWeReadScope(autoSync: boolean): Promise<number> {
  const response = await localFetch(`${BASE}/scope`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ auto_sync: autoSync }),
  });
  if (!response.ok) throw await responseError(response, "保存同步设置失败");
  const body = (await response.json()) as { scope_revision: number };
  return body.scope_revision;
}

export async function startWeReadSync(expectedRevision: number): Promise<number> {
  const response = await localFetch(`${BASE}/sync`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ expected_revision: expectedRevision }),
  });
  if (!response.ok) throw await responseError(response, "启动同步失败");
  const body = (await response.json()) as { run_id: number };
  return body.run_id;
}
