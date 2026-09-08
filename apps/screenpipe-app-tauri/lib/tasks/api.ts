// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

import { localFetch } from "@/lib/api";
import type {
  TaskControl,
  TaskDefinition,
  TaskDefinitionUpdate,
  TaskEvent,
  TaskRun,
  TaskRunSummary,
  TaskTrigger,
} from "./types";

async function parse<T>(res: Response, fallback: string): Promise<T> {
  const body = (await res.json().catch(() => ({}))) as T & {
    message?: string;
    code?: string;
  };
  if (!res.ok) {
    const error = new Error(body.message ?? `${fallback} (${res.status})`) as Error & {
      code?: string;
      status?: number;
    };
    error.code = body.code;
    error.status = res.status;
    throw error;
  }
  return body;
}

export async function listTaskDefinitions(): Promise<TaskDefinition[]> {
  const response = await localFetch("/tasks/definitions");
  const body = await parse<{ definitions: TaskDefinition[] }>(response, "加载自动化定义失败");
  return body.definitions;
}

export async function updateTaskDefinition(
  definitionId: string,
  request: TaskDefinitionUpdate & { expected_revision: number },
): Promise<TaskDefinition> {
  const response = await localFetch(
    `/tasks/definitions/${encodeURIComponent(definitionId)}`,
    {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
    },
  );
  const body = await parse<{ definition: TaskDefinition }>(
    response,
    "更新自动化定义失败",
  );
  return body.definition;
}

export async function startTaskRun(
  definitionId: string,
  request: {
    trigger_key?: string;
    input_hash?: string;
    input_refs?: Record<string, unknown>;
    config_snapshot?: Record<string, unknown>;
    priority?: number;
    deadline?: string;
  } = {},
): Promise<{ run_id: string; created: boolean; compatibility?: string }> {
  const response = await localFetch(
    `/tasks/definitions/${encodeURIComponent(definitionId)}/runs`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
    },
  );
  return parse(response, "启动自动化失败");
}

export async function listTaskRuns(limit = 50): Promise<TaskRunSummary[]> {
  const response = await localFetch(`/tasks/runs?limit=${Math.max(1, Math.min(limit, 200))}`);
  const body = await parse<{ runs: TaskRunSummary[] }>(response, "加载自动化运行失败");
  return body.runs;
}

export async function getTaskRun(runId: string): Promise<TaskRun> {
  const response = await localFetch(`/tasks/runs/${encodeURIComponent(runId)}`);
  const body = await parse<{ run: TaskRun }>(response, "加载自动化运行详情失败");
  return body.run;
}

export async function listTaskEvents(
  runId: string,
  afterSeq = 0,
  limit = 100,
): Promise<TaskEvent[]> {
  const response = await localFetch(
    `/tasks/runs/${encodeURIComponent(runId)}/events?after_seq=${afterSeq}&limit=${Math.max(1, Math.min(limit, 1000))}`,
  );
  const body = await parse<{ events: TaskEvent[] }>(response, "加载自动化事件失败");
  return body.events;
}

export async function controlTaskRun(
  runId: string,
  control: TaskControl,
  expectedRevision: number,
): Promise<TaskRun> {
  const response = await localFetch(`/tasks/runs/${encodeURIComponent(runId)}/control`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ control, expected_revision: expectedRevision }),
  });
  const body = await parse<{ run: TaskRun }>(response, "更新自动化运行状态失败");
  return body.run;
}
