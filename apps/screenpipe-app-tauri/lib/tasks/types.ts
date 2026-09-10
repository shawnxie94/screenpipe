// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

/** Hand-mirrored public task REST contract (screenpipe-core/src/tasks). */
export type TaskKind =
  | "knowledge_extract"
  | "knowledge_compile"
  | "knowledge_backfill"
  | "history_migration"
  | "activity_summary"
  | "office_sync"
  | "pipe_run";

export type TaskOrigin = "builtin" | "user" | "connection";
export type TaskState =
  | "queued"
  | "running"
  | "succeeded"
  | "failed"
  | "timed_out"
  | "cancelling"
  | "cancelled"
  | "paused"
  | "needs_attention";

export type TaskResourceClass =
  | "interactive"
  | "extract"
  | "backfill"
  | "office_io"
  | "user_pipe";

export interface TaskRetryPolicy {
  max_attempts: number;
  network_retries: number;
  invalid_output_retries: number;
  backoff_ms: number;
}

export interface TaskTrigger {
  kind: string;
  expression?: string;
}

export interface TaskDefinition {
  definition_id: string;
  kind: TaskKind;
  origin: TaskOrigin;
  /** True when connection configuration, scope, and trigger own this definition. */
  managed_by_connection?: boolean;
  /** UI target for editing the owning connection rather than the task itself. */
  configuration_target?: string;
  /** True when a user Pipe's pipe.md remains the sole configuration source. */
  managed_by_pipe?: boolean;
  schema_version: number;
  config_revision: string;
  config_ref?: string;
  trigger: TaskTrigger;
  enabled: boolean;
  resource_class: TaskResourceClass;
  retry_policy: TaskRetryPolicy;
  model_binding_policy?: string;
  revision: number;
  owner_generation: number;
  migration_state: string;
}

export interface TaskDefinitionUpdate {
  enabled?: boolean;
  config_revision?: string;
  config_ref?: string;
  trigger?: TaskTrigger;
  model_binding_policy?: string;
}

export interface TaskRun {
  run_id: string;
  definition_id?: string;
  definition_revision: string;
  root_run_id: string;
  parent_run_id?: string;
  retry_of?: string;
  trigger_key: string;
  input_hash: string;
  input_refs: Record<string, unknown>;
  config_snapshot: Record<string, unknown>;
  state: TaskState;
  revision: number;
  priority: number;
  not_before?: string;
  deadline?: string;
  cursor?: string;
  lease_owner?: string;
  lease_expires_at?: string;
  owner_generation: number;
  output_refs: Record<string, unknown>;
  error_code?: string;
}

export interface TaskRunSummary {
  run_id: string;
  definition_id?: string;
  state: TaskState;
  root_run_id: string;
  revision: number;
}

export interface TaskEvent {
  run_id: string;
  seq: number;
  attempt_id?: string;
  phase: string;
  event_type: string;
  timestamp: string;
  safe_metadata: Record<string, unknown>;
  payload_ref?: string;
  output_refs: Record<string, unknown>;
}

export type TaskControl = "pause" | "resume" | "cancel" | "retry";
