// @opake/daemon — background task scheduling for browser environments

export { startDaemon, type DaemonHandle } from "./scheduler";
export type { DaemonOptions, SSEConfig, TaskDef, TaskRecord, TaskStatus, TaskStore } from "./types";
export type { SSEConsumerHandle } from "./sse-consumer";
