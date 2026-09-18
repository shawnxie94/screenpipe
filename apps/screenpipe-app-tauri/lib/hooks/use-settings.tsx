// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

import { homeDir } from "@tauri-apps/api/path";
import { commands } from "@/lib/utils/tauri";
import { platform } from "@tauri-apps/plugin-os";
import { Store } from "@tauri-apps/plugin-store";
import {
	createSettingsWriteQueue,
	enqueueSettingsWrite,
	flushSettingsWrites,
} from "@/components/settings/settings-write-queue";
import React, { createContext, useContext, useEffect, useRef, useState } from "react";
import { type SourceCitation } from "@/lib/source-citations";
import { SettingsStore } from "../utils/tauri";
import { type FontSize, applyFontSize } from "@/lib/utils/font-size";
import {
	clearLegacyUserGoalCategory,
	DEFAULT_USER_GOAL_CATEGORY,
	normalizeUserGoalCategory,
	readLegacyUserGoalCategory,
	type UserGoalCategory,
} from "@/lib/live-views/onboarding-activation";
import {
	DEFAULT_SIDEBAR_NAV_LAYOUT,
	type SidebarNavLayout,
} from "@/lib/utils/sidebar-nav-layout";
export type VadSensitivity = "low" | "medium" | "high";

export type AIProviderType =
	| "native-ollama"
	| "openai"
	| "openai-chatgpt"
	| "anthropic"
	| "custom"
	| "embedded"
	| "acp"
	| "pi";

export type AcpAgentPresetConfig = {
	/** Stable adapter id from the ACP registry, or "custom". */
	id: string;
	/** Only needed for custom/local adapters. Curated adapters are resolved by id. */
	command?: string;
	args?: string[];
	/** Empty values mean "inherit this variable from the app environment". */
	env?: Record<string, string>;
	/** Session option defaults advertised by the selected adapter. */
	config?: Record<string, string>;
	/** Session mode default advertised by the selected adapter. */
	modeId?: string | null;
	/** Screenpipe-owned ACP permission response policy. */
	approvalMode?: "ask" | "allow-all" | null;
};

export type EmbeddedLLMConfig = {
	enabled: boolean;
	model: string;
	port: number;
};

export enum Shortcut {
	SHOW_SCREENPIPE = "show_screenpipe",
	START_RECORDING = "start_recording",
	STOP_RECORDING = "stop_recording",
}

export type AIPreset = {
	id: string;
	maxContextChars: number;
	maxTokens?: number;
	url: string;
	model: string;
	defaultPreset: boolean;
	prompt: string;
} & (
	| {
			provider: "openai";
			apiKey: string;
	  }
	| {
			provider: "native-ollama";
	  }
	| {
			provider: "acp";
			acpAgent: AcpAgentPresetConfig;
	  }
	| {
			provider: "anthropic";
			apiKey: string;
	  }
	| {
			provider: "custom";
			apiKey: string;
	  }
	| {
			provider: "pi";
	  }
	| {
			provider: "openai-chatgpt";
	  }
);

// Chat history types
export interface ChatMessage {
	id: string;
	role: "user" | "assistant";
	content: string;
	/** Local history source for messages copied from another agent client. */
	importedFrom?: "claude-code" | "codex";
	intent?: "steer";
	turnIntentId?: string;
	timestamp: number;
	contentBlocks?: any[];
	sourceCitations?: SourceCitation[];
	model?: string;
	provider?: string;
	/** UI override — when set, the sidebar / panel header renders this
	 *  instead of `content` for compact display (e.g. "pipe executed
	 *  10:24 – 10:26" for synthetic prompts). Doesn't affect persistence
	 *  or what's sent to the model. */
	displayContent?: string;
	images?: any[];
	/** Non-image attachments (PDF/DOCX/XLSX/text) extracted to text. Only
	 *  metadata is stored here — the actual extracted text already lives
	 *  inside `content` (folded in at send time so the model sees it).
	 *  The renderer reads this to draw attachment cards above the bubble. */
	attachments?: Array<{
		name: string;
		ext: string;
		charCount: number;
		truncated: boolean;
	}>;
	interruptedBySteer?: boolean;
	steeredResponse?: boolean;
	stoppedByUser?: boolean;
	interruptedByQuit?: boolean;
	/** Wall-clock work duration for coalesced assistant messages (pipe
	 *  runs). Used by the chat renderer as a fallback when no thinking
	 *  blocks contributed a duration, so the work-group can still show
	 *  "Worked for X min" even when the agent emitted no thinking. */
	workDurationMs?: number;
}

/** What kind of session a conversation represents.
 *
 *  - `chat`        — a normal Pi chat session. The default; assumed when
 *                    `kind` is missing on disk.
 *  - `pipe-watch`  — a live pipe execution the user is currently
 *                    watching. The chat panel renders pipe events in
 *                    real time; the conversation is volatile (not
 *                    persisted unless the user opts to keep it).
 *  - `pipe-run`    — a completed pipe execution kept around as
 *                    history. Lives under "Pipe runs" in the sidebar
 *                    rather than "Recents". */
export type ConversationKind = "chat" | "pipe-watch" | "pipe-run";

/** The client surface that hosted an imported agent conversation. */
export type AgentHarness = "terminal" | "cursor" | "github-copilot" | "screenpipe";

/** Pipe-specific context attached to `pipe-watch` / `pipe-run`
 *  conversations. Drives the in-panel banner and the sidebar
 *  grouping. */
export interface PipeContext {
	pipeName: string;
	executionId: number;
	startedAt?: string;
}

export interface ChatConversation {
	id: string;
	title: string;
	messages: ChatMessage[];
	createdAt: number;
	updatedAt: number;
	/** Provenance for an explicitly imported local agent conversation. */
	importedFrom?: {
		source: "claude-code" | "codex";
		sourceId: string;
		importedAt: number;
		/** Optional when the transcript exposes which client hosted the run. */
		harness?: AgentHarness;
	};
	/** User pinned this conversation in the chat sidebar — keeps it at the top.
	 *  Persists across app restarts via the on-disk conversation file. */
	pinned?: boolean;
	/** User closed this conversation from the chat sidebar — keeps the file on
	 *  disk (so deleting via close is non-destructive) but excludes it from the
	 *  sidebar listing. Re-surface via a future "show hidden" UI; meanwhile a
	 *  dedicated delete-forever action is the only way to actually remove. */
	hidden?: boolean;
	/** ms since epoch of the most recent USER-SENT message. Drives the
	 *  sidebar sort order. Persisted so that order survives app restart;
	 *  derived from messages on first hydration if not set on disk yet. */
	lastUserMessageAt?: number;
	/** ms since epoch of the most recent actual message append (user or
	 *  assistant). Drives unread detection — immune to non-content writes. */
	lastContentAt?: number;
	/** ms since epoch of the most recent time this chat was actually opened.
	 *  A value of `0` means "never viewed" for persisted unread restore. */
	lastViewedAt?: number;
	/** Conversation type — defaults to "chat" when missing (back-compat
	 *  with older on-disk files). See `ConversationKind`. */
	kind?: ConversationKind;
	/** Pipe metadata for `pipe-watch` / `pipe-run` conversations.
	 *  Undefined for plain chats. */
	pipeContext?: PipeContext;
	/** Optional user-assigned sidebar group label (lowercase, trimmed).
	 *  Drives the manual-grouping sections in the chat sidebar. */
	sidebarGroup?: string;
	/** Id of the conversation this one was branched from. Set only by
	 *  "branch in new chat". Exempts the branch from first-user-message
	 *  dedup — it deliberately shares its parent's opening message. */
	branchedFrom?: string;
	/** Monotonic write counter, bumped by `saveConversationFile` on every
	 *  persist. Drives compare-and-swap: a writer whose `rev` is behind the
	 *  on-disk copy lost a race, so its save merges instead of overwriting.
	 *  Absent on files written before CAS landed — treated as 0, which makes
	 *  legacy writers merge rather than clobber. */
	rev?: number;
	/** Last URL the agent navigated the embedded browser sidebar to.
	 *  Drives the right-side `<BrowserSidebar />` panel: when the user
	 *  re-opens this conversation the panel restores to this URL.
	 *  Cleared (set to undefined) when the user closes the sidebar. */
	browserState?: {
		url: string;
		updatedAt: number;
		/** User-chosen panel width in CSS pixels. Defaults to 480 if unset.
		 *  Persisted so re-opening the chat restores the same layout. */
		width?: number;
		/** User has hidden the panel (still has a saved URL — a small
		 *  "re-open" button is shown in the chat header). */
		collapsed?: boolean;
	};
	/** Title source priority: user > ai > fallback. Used to prevent
	 *  lower-priority titles from overwriting higher-priority ones. */
	titleSource?: "user" | "ai" | "fallback";
	/** The AI preset ID last used in this conversation. Used to restore
	 *  the model selection when switching between chats. Persisted to disk
	 *  so the selection survives app restart. */
	presetId?: string;
	/** The last live ACP session id for this conversation. Persisted so a
	 *  reopen after the agent process is gone can reattach to that session
	 *  (session/resume) instead of starting fresh. */
	acpSessionId?: string;
}

export interface ChatHistoryStore {
	conversations: ChatConversation[];
	activeConversationId: string | null;
	historyEnabled: boolean;
}

// Extend SettingsStore with fields added before Rust types are regenerated
export type Settings = SettingsStore & {
	/** Enable automatic Activities generation. Default false. */
	activitiesEnabled?: boolean;
	/** Native Activity generation cadence in minutes. Default 15. */
	activitiesIntervalMinutes?: number;
	/** ① Activity interval rebuild cadence in minutes (knowledge pipeline). Default 5, range 1–60. */
	knowledgeReconcileMinutes?: number;
	/** ① Rebuild lookback window in hours. Default 2, range 1–48. */
	knowledgeReconcileWindowHours?: number;
	/** ② Interval summary scan cadence in minutes. Default 15, range 5–1440. */
	knowledgeSummarizeMinutes?: number;
	/** ③ WorkUnit extraction scan cadence in minutes. Default 60, range 5–1440. */
	knowledgeWorkUnitMinutes?: number;
	/** ④ Knowledge distillation period in hours. Default 24, range 1–168. */
	knowledgeDistillHours?: number;
	/** ④ Cooldown before the same scope is re-distilled, in days. Default 7, range 1–90. */
	knowledgeDistillCooldownDays?: number;
	/** ① Same-object merge gap in minutes. Default 10, range 1–120. */
	activityMergeGapMinutes?: number;
	/** ① Short-segment dwell floor in seconds. Default 30, range 5–300. */
	activityMinDwellSeconds?: number;
	/** ②③ Discovery lookback window in hours. Default 26, range 1–168. */
	knowledgeDiscoveryLookbackHours?: number;
	/** ②③ Max jobs enqueued per discovery round. Default 20, range 1–500. */
	knowledgeDiscoveryBatch?: number;
	/** Single model-call timeout in seconds. Default 45, range 10–600. */
	knowledgeCallTimeoutSeconds?: number;
	/** Per-job total step timeout in seconds. Default 120, range 30–1800. */
	knowledgeStepTimeoutSeconds?: number;
	/** Max model calls per job. Default 3, range 1–10. */
	knowledgeMaxModelCallsPerJob?: number;
	/** AI preset used by native Activity generation. */
	activitiesAiPresetId?: string;
	/** Next native Activity generation run as an ISO timestamp. */
	activitiesNextRunAt?: string;
	/** Goal used to prioritize the Home cards. Persisted in store.bin. */
	userGoalCategory?: UserGoalCategory;
	/** Stable local identifier used for device-scoped behavior. */
	deviceId?: string;
	chatHistory?: ChatHistoryStore;
	/**
	 * Entries the capture-category switches created, so turning a category off
	 * removes only those and never a rule the user wrote by hand.
	 */
	categoryOwnedFilters?: { apps: string[]; domains: string[] };
	searchShortcut?: string;
	lockVaultShortcut?: string;
	/** When true, audio devices follow system default and auto-switch on changes */
	useSystemDefaultAudio?: boolean;
	/** Enable AI workflow event detection (cloud, triggers event-based pipes) */
	/** Audio transcription scheduling: "batch" (default, longer chunks for quality) or "realtime". */
	transcriptionMode?: "realtime" | "smart" | "batch";
	/** Live notes for manually-started meetings. Separate from background 24/7 transcription. */
	meetingLiveTranscriptionEnabled?: boolean;
	/** Provider for manually-started live notes. Defaults to the selected transcription engine. */
	meetingLiveTranscriptionProvider?: "selected-engine" | "disabled" | "deepgram-live";
	/** When true, the user's typed text (and edited files) captured during a meeting is auto-appended to the meeting note when the meeting stops. Default true. */
	appendTypedTextToMeetingNote?: boolean;
	/** User's name for speaker identification — input device audio will be labeled with this name */
	userName?: string;
	/** Custom vocabulary entries for transcription biasing and word replacement */
	vocabularyWords?: Array<{ word: string; replacement?: string }>;
	/** Slug of the pipe used to summarize meetings. Drives both the manual
	 * "Summarize with AI" button (its body becomes the chat prompt) and the
	 * auto-fire on meeting_ended (the picked pipe owns the trigger). Default:
	 * "meeting-summary" (the built-in pipe). */
	meetingSummaryPipeSlug?: string;
	/** Font size for the entire app UI */
	fontSize?: FontSize;
	/** OpenAI-compatible transcription endpoint URL */
	openaiCompatibleEndpoint?: string;
	/** OpenAI-compatible transcription API key */
	openaiCompatibleApiKey?: string;
	/** OpenAI-compatible transcription model name */
	openaiCompatibleModel?: string;
	/** Custom HTTP headers for OpenAI-compatible transcription (JSON object) */
	openaiCompatibleHeaders?: Record<string, string>;
	/** Send raw WAV audio instead of MP3 to OpenAI-compatible endpoint */
	openaiCompatibleRawAudio?: boolean;
	/** Filter music-dominant audio before transcription (reduces Spotify/YouTube music noise) */
	filterMusic?: boolean;
	/** Maximum batch transcription duration in seconds (0 = engine default: Deepgram 5000s, OpenAI 3000s, Whisper 600s) */
	batchMaxDurationSecs?: number;
	/** User's power mode preference — persisted so it survives app restarts */
	powerMode?: "auto" | "performance" | "battery_saver";
	/** Show restart notifications when audio/vision capture stalls (default: false for now) */
	showRestartNotifications?: boolean;
	/** Hide only overlay windows from screen recordings and sharing. Default false. */
	hideOverlayInScreenRecording?: boolean;
	/** @deprecated Retained for settings compatibility. */
	hideAppInScreenShare?: boolean;
	/** Pause all screen capture when a DRM-protected streaming app (Netflix, Disney+, etc.) or a remote-desktop client (Omnissa/VMware Horizon) is focused — they blank their windows during screen recording */
	pauseOnDrmContent?: boolean;
	/** Skip clipboard capture in the UI recorder (events + content). Defaults to true (clipboard capture OFF) — passwords / API keys often pass through the clipboard, so it's opt-in. */
	disableClipboardCapture?: boolean;
	/** Skip keyboard / typed-text capture in the UI recorder. Defaults to true (keyboard capture OFF) — the a11y tree + OCR still capture on-screen text, this only drops the raw keystroke stream where secrets get typed. */
	disableKeyboardCapture?: boolean;
	/** Skip mouse-click rows in the UI recorder. Defaults to false (click capture ON) — clicks carry no text payload and drive workflow/task mining. Clicks still wake event-driven capture when disabled. */
	disableClickCapture?: boolean;
	/** Capture System Audio via CoreAudio Process Tap on macOS 14.4+ instead of ScreenCaptureKit.
	 *  Desktop migration V3 enables it automatically. Initial tap failures fall back to SCK;
	 *  ignored on macOS <14.4 and non-macOS. */
	experimentalCoreaudioSystemAudio?: boolean;
	/** Beta ("Smart recording" in the app): during meetings, capture only the meeting app's audio
	 *  and the microphone it actually uses (per-process piggyback). Off by default. Engages in ANY
	 *  audio capture mode — takes precedence over the configured devices for the meeting's
	 *  duration. Requires the meeting detector. Falls back to standard capture automatically if
	 *  unavailable. */
	experimentalMeetingPiggyback?: boolean;
	/** Opening a Bluetooth mic always degrades the paired device's output audio (A2DP -> SCO,
	 *  a macOS/OS limitation — issue #3750). Off by default: Bluetooth mics are only recorded
	 *  during a detected meeting. Turn on to always record Bluetooth mics regardless of
	 *  meeting state. No effect on wired/built-in mics, Bluetooth output devices, or a dedicated
	 *  Bluetooth mic with no output side of its own — nothing to protect there. */
	alwaysRecordBluetoothMic?: boolean;
	/** Experimental: request Windows WASAPI microphone AEC when supported. */
	windowsInputAecEnabled?: boolean;
	/** Experimental: request Apple VoiceProcessingIO AEC on the default macOS microphone. */
	macosInputVpioEnabled?: boolean;
	/** Request Screenpipe's software Acoustic Echo Cancellation (via sonora WebRTC AEC3). */
	screenpipeAecEnabled?: boolean;
	/** Selected echo cancellation engine. Missing values default to off. */
	aecMode?: "off" | "screenpipe" | "macos" | "windows";
	/** Continue recording audio when the screen is locked (default: false) */
	recordWhileLocked?: boolean;
	/** Auto-delete local data older than retention days (free alternative to cloud archive) */
	localRetentionEnabled?: boolean;
	/** Days to keep data locally before auto-deleting (default: 14) */
	localRetentionDays?: number;
	/** What gets deleted past the cutoff:
	 * - "media" (default): keep all DB rows (search/timeline still work), only
	 *   reclaim mp4/wav/jpeg files on disk.
	 * - "lean": also strip the heavy accessibility/OCR element tree, the raw AX
	 *   tree JSON, and the ui_events stream — shrinks the database itself while
	 *   keeping text, transcripts, and memories searchable.
	 * - "all": wipe everything past the cutoff. */
	localRetentionMode?: "media" | "lean" | "all";
	/** Apply macOS vibrancy effect to sidebar for a translucent glass look */
	translucentSidebar?: boolean;
	/** User-customized Home sidebar: row order plus the ids kept out of it.
	 *  Meetings ships hidden, which is what puts its compact icon in the
	 *  top-left chrome strip instead. See `lib/utils/sidebar-nav-layout`. */
	sidebarNavLayout?: SidebarNavLayout;
	/** Show the chat suggestion chips above the input — the "follow up"
	 *  questions and the connection-aware suggested prompts. The single inline
	 *  X on the chips flips this to false; re-enable from Settings → Display.
	 *  Default true. */
	showChatSuggestions?: boolean;
	/** Auto-generate chat titles with the LLM after the first message.
	 *  Costs one extra inference per new chat. Disable to save tokens —
	 *  chats fall back to a title derived from the first message (default: true) */
	autoGenerateChatTitles?: boolean;
	/** Notification preferences — which notification sources are enabled.
	 *  The set of per-category keys is declared in
	 *  `components/settings/notification-registry.ts` (single source of truth);
	 *  this type lists the stable ones the Rust side reads by name. */
	notificationPrefs?: {
		/** Master switch. When false the `/notify` handler drops every panel
		 *  (pipe, system, power, transcript-stall, …). Default true. */
		notificationsEnabled?: boolean;
		/** Snooze expiry as epoch ms. While `> Date.now()` all non-critical
		 *  notifications are paused. 0 / unset = not snoozed. */
		snoozeUntil?: number;
		/** Recurring daily quiet window (local wall-clock). When `enabled`,
		 *  non-critical notifications are paused inside the window. */
		quietHours?: { enabled: boolean; start: string; end: string };
		/** Pipe names that still notify while snoozed / in quiet hours (Slack-VIP
		 *  pattern). A hard master-off still silences them. */
		allowDuringPause?: string[];
		captureStalls: boolean;
		appUpdates: boolean;
		pipeNotifications: boolean;
		/** In-app /notify before background scheduled tasks burn most of hosted-AI allowance.
		 *  Default true; still gated by master notifications and pipe notifications. */
		pipeAllowanceWarnings?: boolean;
		/** Toast when a monitor is plugged, unplugged, or switched (clamshell, dock). Default true. */
		displayChanges?: boolean;
		/** Live-note prompt when a meeting is detected. Default true. */
		meetingLiveNotes?: boolean;
		/** OS notification when a meeting starts but no audio frames arrive within 60s. Default true. */
		audioCaptureStalled?: boolean;
		/** In-app /notify when audio is captured but no live transcript arrives within 60s. Default true. */
		liveTranscriptStalled?: boolean;
		/** Toast on informational power-profile transitions (Balanced / Saver), e.g. when unplugging AC.
		 *  Critical AudioPaused/FullPause alerts always fire regardless. Default true. */
		powerModeChanges?: boolean;
		mutedPipes: string[];
	};
	/** Remote devices to monitor pipes on (LAN addresses) */
		monitorDevices?: Array<{
			address: string;
			label?: string;
		}>;
		/** Enable recording schedule — when on, recording only runs during defined time ranges */
		scheduleEnabled?: boolean;
	/** Per-day-of-week time ranges defining when recording is active */
	scheduleRules?: Array<{
		dayOfWeek: number;
		startTime: string;
		endTime: string;
		recordMode: string;
	}>;
	apiAuth?: boolean;
	apiKey?: string;
	/** Default behavior when a meeting is detected.
	 * - `"ask"` (default): the existing meeting-start notification grows
	 *   a "+ HD" action. Click → starts a meeting-bound session that
	 *   auto-stops when the call ends.
	 * - `"always"`: every detected meeting auto-starts a session.
	 * - `"never"`: no auto-action; only the manual tray timer can start
	 *   one.
	 * Indefinite manual mode does not exist — every session is bound to
	 * either a meeting or a timer, both with hard-cap safety nets. */
	hdRecordingDefault?: "ask" | "always" | "never";
	/** Capture debounce (ms) installed while an HD session is active.
	 * Default 100 ≈ 10 fps. Clamped to >= 33 ms (30 fps ceiling). */
	hdRecordingIntervalMs?: number;
	/**
	 * When true the backend binds the HTTP API to 0.0.0.0 instead of 127.0.0.1
	 * so other devices on the LAN can reach it. api_auth is force-enabled
	 * whenever this is true — the backend mirrors the guard in
	 * RecordingConfig::from_settings so the two flags stay consistent even
	 * if someone edits the settings file by hand.
	 */
	listenOnLan?: boolean;
	encryptStore?: boolean;
	/** Global blanket permission: allow screenpipe to copy browser cookies
	 *  into the owned browser so the agent can browse sites the user is
	 *  logged into. Revocable from the owned-browser cookie menu.
	 *  Undefined = not decided yet, false = disabled, true = enabled. */
	browserCookieAccessGranted?: boolean;
	/** Windows-only: when true, closing the Home window hides it to the system
	 * tray (and removes it from the taskbar) instead of minimizing. The Rust
	 * close handler in src-tauri/src/main.rs reads this directly. Default off. */
	minimizeToTrayOnClose?: boolean;
	/** macOS-only: when true, run as a menu-bar-only "agent" app with no Dock
	 * icon (NSApplication Accessory activation policy). The tray icon stays
	 * visible. Read by reset_to_regular_and_refresh_tray in
	 * src-tauri/src/window/panel.rs at startup and on window events. Default off. */
	hideDockIcon?: boolean;
	/** True once the in-app first-run guide (shown after onboarding on the Home
	 * window) has been completed or skipped. Stored here so it persists in the
	 * normal settings store with no bindings regen. Default off. */
	firstRunGuideDone?: boolean;
}

export function getEffectiveFilters(settings: Settings) {
	return {
		ignoredWindows: [...new Set(settings.ignoredWindows)],
		includedWindows: [...new Set(settings.includedWindows)],
		ignoredUrls: [...new Set(settings.ignoredUrls || [])],
	};
}

export const DEFAULT_PROMPT = `规则：
- 媒体：使用带尖括号本地路径的标准 Markdown，例如视频使用 ![描述](</path/to/file.mp4>)，图片使用 ![描述](</path/to/image.jpg>)
- 本地文件路径始终放在尖括号中，因为 screenpipe 路径经常包含空格或括号
- 图表：使用 \`\`\`mermaid 代码块制作可视化摘要（流程图、甘特图、思维导图、关系图）
- 活动摘要：使用展示应用和时长的甘特图
- 工作流：使用展示执行步骤的流程图
- 知识来源：使用关系图展示信息来自哪里（应用、时间、对话）
- 会议：提取说话人、决策和行动项
- 保持事实性，只使用提供的数据
`;

/** Replace the English built-in prompt left by older installs, but preserve
 * any prompt the user wrote themselves. */
export function localizedPresetPrompt(prompt?: string): string {
  if (
    !prompt ||
    (prompt.startsWith("Rules:\n- Media: use standard markdown") &&
      prompt.includes("Always wrap local file paths in angle brackets"))
  ) {
    return DEFAULT_PROMPT;
  }
  return prompt;
}

const DEFAULT_IGNORED_WINDOWS_IN_ALL_OS = [
	"bit",
	"VPN",
	"Trash",
	"Private",
	"Incognito",
	"Wallpaper",
	"Settings",
	"Keepass",
	"Recorder",
	"vault",
	"OBS Studio",
	"screenpipe",
];

const DEFAULT_IGNORED_WINDOWS_PER_OS: Record<string, string[]> = {
	macos: [
		".env",
		"Item-0",
		"App Icon Window",
		"Battery",
		"Shortcuts",
		"WiFi",
		"BentoBox",
		"Clock",
		"Dock",
		"DeepL",
		"Control Center",
	],
	windows: ["Nvidia", "Control Panel", "System Properties"],
	linux: ["Info center", "Discover", "Parted"],
};

// Local-only builds never assume a provider or endpoint. The user chooses an
// installed Pi Runtime provider or configures a third-party provider.
export function makeDefaultPresets(): AIPreset[] {
	return [];
}

const DEFAULT_AUDIO_ENGINE = "qwen3-asr";

let DEFAULT_SETTINGS: Settings = {
			activitiesEnabled: true,
			activitiesIntervalMinutes: 15,
			knowledgeReconcileMinutes: 5,
			knowledgeReconcileWindowHours: 2,
			knowledgeSummarizeMinutes: 15,
			knowledgeWorkUnitMinutes: 60,
			knowledgeDistillHours: 24,
			knowledgeDistillCooldownDays: 7,
			activityMergeGapMinutes: 10,
			activityMinDwellSeconds: 30,
			knowledgeDiscoveryLookbackHours: 26,
			knowledgeDiscoveryBatch: 20,
			knowledgeCallTimeoutSeconds: 45,
			knowledgeStepTimeoutSeconds: 120,
			knowledgeMaxModelCallsPerJob: 3,
			aiPresets: makeDefaultPresets() as any,
			userGoalCategory: DEFAULT_USER_GOAL_CATEGORY,
			deviceId: crypto.randomUUID(),
			deepgramApiKey: "",
			isLoading: false,
			userId: "",
			devMode: false,
			audioTranscriptionEngine: "qwen3-asr",
			meetingLiveTranscriptionEnabled: true,
			meetingLiveTranscriptionProvider: "selected-engine",
			appendTypedTextToMeetingNote: true,
			ocrEngine: "default",
			monitorIds: ["default"],
			audioDevices: ["default"],
			useSystemDefaultAudio: true,
			// Default ON (#3819): this is the lightweight hot-path regex redaction
			// in screenpipe-core (emails, phone numbers, SSNs, card numbers, API
			// keys, etc.) — NOT the heavy async AI model (asyncPiiRedaction stays
			// off, so no ~2.8GB model download). Privacy-by-default for new installs;
			// existing users keep whatever they already chose.
			usePiiRemoval: true,
			port: 3030,
			dataDir: "default",
			disableAudio: false,
			// New installs capture audio only during detected meetings (saves cloud
			// transcription cost, disk, and CPU). Existing installs are NOT backfilled
			// — they have no stored value, so the serde/UI "always" fallback keeps them
			// on continuous capture without rewriting their settings.
			audioCaptureMode: "meetings-only",
			ignoredWindows: [
			],
			includedWindows: [],
			ignoredUrls: [],
			includedUrls: [],
			ignoredMeetingApps: [],

			audioChunkDuration: 30,
			useChineseMirror: false,
			languages: [],
			embeddedLLM: {
				enabled: false,
				model: "ministral-3:latest",
				port: 11434,
			},
			autoUpdatePipes: true,
			autoStartEnabled: true,
			platform: "unknown",
			disabledShortcuts: [],
			showScreenpipeShortcut: "Control+Super+S",
			startRecordingShortcut: "Super+Alt+U",
			stopRecordingShortcut: "Super+Alt+X",
			startAudioShortcut: "Control+Super+A",
			stopAudioShortcut: "Control+Super+Z",
			showChatShortcut: "Control+Super+L",
			searchShortcut: "Control+Super+K",
			lockVaultShortcut: "Super+Shift+L",
			disableVision: false,
			disableScreenshots: false,
			enableSemanticContext: false,
			semanticContextMode: "memory",
			useAllMonitors: true,
			chatHistory: {
				conversations: [],
				activeConversationId: null,
				historyEnabled: true,
			},
			overlayMode: "fullscreen",
			showOverlayInScreenRecording: false,
			hideOverlayInScreenRecording: false,
			hideAppInScreenShare: true,
			disableTimeline: false,
			firstRunGuideDone: false,
			videoQuality: "balanced",
			transcriptionMode: "batch",
			meetingSummaryPipeSlug: "meeting-summary",
			filterMusic: true,
			prioritizeInputLatency: false,
			allowHidingShortcutOverlay: false,
			showShortcutOverlay: true,
			shortcutOverlaySnoozedUntil: null,
			sidebarNavLayout: { ...DEFAULT_SIDEBAR_NAV_LAYOUT },
			ignoreIncognitoWindows: true,
			enhancedIncognitoDetection: false,
			pauseOnDrmContent: false,
			disableClipboardCapture: true,
			disableKeyboardCapture: true,
			disableClickCapture: false,
			keepComputerAwake: false,
			showRestartNotifications: false,
			experimentalCoreaudioSystemAudio: true,
			// Upstream derives this from LOCAL_DESKTOP_REMOTE_POLICY (remote-control
			// policy module, not on this branch); its resolved value is the shipped
			// default `true`.
			experimentalMeetingPiggyback: true,
			alwaysRecordBluetoothMic: false,
			windowsInputAecEnabled: false,
			macosInputVpioEnabled: false,
			screenpipeAecEnabled: false,
			aecMode: "off",
			recordWhileLocked: false,
			localRetentionEnabled: false,
			localRetentionDays: 14,
			localRetentionMode: "media",
			encryptStore: true,
			hdRecordingDefault: "ask",
			hdRecordingIntervalMs: 100,
			headless: false,
			headlessRecordOnly: false,
			fontSize: "16px",
		};

export function createDefaultSettingsObject(): Settings {
	try {
		const p = platform();
		DEFAULT_SETTINGS.platform = p;
		DEFAULT_SETTINGS.ignoredWindows = [...DEFAULT_IGNORED_WINDOWS_IN_ALL_OS];
		DEFAULT_SETTINGS.ignoredWindows.push(...(DEFAULT_IGNORED_WINDOWS_PER_OS[p] ?? []));
		DEFAULT_SETTINGS.ocrEngine = p === "macos" ? "apple-native" : p === "windows" ? "windows-native" : "tesseract";
		DEFAULT_SETTINGS.showScreenpipeShortcut = p === "windows" ? "Alt+S" : "Control+Super+S";
		DEFAULT_SETTINGS.showChatShortcut = p === "windows" ? "Alt+L" : "Control+Super+L";
		DEFAULT_SETTINGS.searchShortcut = p === "windows" ? "Alt+K" : "Control+Super+K";
		DEFAULT_SETTINGS.startAudioShortcut = p === "windows" ? "Alt+Shift+A" : "Control+Super+A";
		DEFAULT_SETTINGS.stopAudioShortcut = p === "windows" ? "Alt+Shift+Z" : "Control+Super+Z";
		DEFAULT_SETTINGS.lockVaultShortcut = p === "windows" ? "Ctrl+Shift+L" : "Super+Shift+L";

		if (p === "windows") {
			DEFAULT_SETTINGS.overlayMode = "window";
		}

		if (p === "linux") {
			DEFAULT_SETTINGS.overlayMode = "window";
		}

		return DEFAULT_SETTINGS;
	} catch (e) {
		// Fallback if platform detection fails
		return DEFAULT_SETTINGS;
	}
}

export function normalizeSettingsArrays(settings: Settings): boolean {
	const defaults = {
		...createDefaultSettingsObject(),
		aiPresets: makeDefaultPresets(),
	};
	let changed = false;
	if (!Array.isArray(settings.aiPresets)) {
		// Local-only installs start with no presets; the user picks a provider.
		settings.aiPresets = [] as any;
		changed = true;
	}

	for (const [key, fallback] of Object.entries(defaults)) {
		if (key === "aiPresets") continue;
		if (!Array.isArray(fallback) || Array.isArray(settings[key])) continue;
		settings[key] = [...fallback];
		changed = true;
	}

	return changed;
}

export function assertValidAiPresetUpdate(value: Partial<Settings>): void {
	if (
		"aiPresets" in value &&
		(!Array.isArray(value.aiPresets) || value.aiPresets.length === 0)
	) {
		throw new Error("At least one AI preset is required");
	}
}

// Store singleton
let _store: Promise<Store> | undefined;

// Settings writes are whole-object read/merge/save operations. Keep them in one
// FIFO so two controls cannot both read the same snapshot and let the slower
// save erase the faster one. The updater also drains this queue before a
// banner-triggered relaunch, which closes the "toggle Auto-update, then click
// Restart to update" race where process exit could beat the preference save.
const settingsWriteQueue = createSettingsWriteQueue();

async function waitForE2eSettingsWriteDelay(): Promise<void> {
	if (
		process.env.NEXT_PUBLIC_SCREENPIPE_E2E !== "true" ||
		typeof document === "undefined"
	) {
		return;
	}
	const delayMs = Number(
		document.documentElement.dataset.e2eSettingsWriteDelayMs ?? 0,
	);
	if (Number.isFinite(delayMs) && delayMs > 0) {
		await new Promise((resolve) => setTimeout(resolve, delayMs));
	}
}

function enqueueSettingsStoreWrite(write: () => Promise<void>): Promise<void> {
	const queuedWrite = async () => {
		await waitForE2eSettingsWriteDelay();
		await write();
		if (
			process.env.NEXT_PUBLIC_SCREENPIPE_E2E === "true" &&
			typeof document !== "undefined"
		) {
			document.documentElement.dataset.e2eSettingsWriteFinishedAt = String(
				performance.now(),
			);
		}
	};
	return enqueueSettingsWrite(settingsWriteQueue, queuedWrite);
}

export async function flushPendingSettingsWrites(): Promise<void> {
	await flushSettingsWrites(settingsWriteQueue);
}

export const getStore = async () => {
	if (!_store) {
		_store = (async () => {
			// Resolve the base dir via the backend so the webview opens the same
			// store.bin as Rust (get_base_dir honors SCREENPIPE_DATA_DIR); a
			// hardcoded ~/.screenpipe here splits the settings store in two
			// whenever that override is set.
			let baseDir: string | null = null;
			try {
				const res = await commands.getScreenpipeBaseDir();
				if (res.status === "ok") {
					baseDir = res.data;
				} else {
					console.warn("get_screenpipe_base_dir failed, using ~/.screenpipe:", res.error);
				}
			} catch (e) {
				console.warn("get_screenpipe_base_dir unavailable, using ~/.screenpipe:", e);
			}
			if (!baseDir) {
				baseDir = `${await homeDir()}/.screenpipe`;
			}
			return Store.load(`${baseDir}/store.bin`, {
				autoSave: false,
				defaults: {},
			});
		})();
	}
	return _store;
};

/** Save the store and re-encrypt store.bin on disk (keychain encryption). */
export const saveAndEncrypt = async (store: Store) => {
	await store.save();
	await commands.reencryptStore().catch(() => {});
};

async function persistSettings(store: Store, settings: Settings) {
	normalizeSettingsArrays(settings);
	await store.set("settings", settings);
}

// Store utilities similar to Cap's implementation
function createSettingsStore() {
	const get = async (): Promise<Settings> => {
		const store = await getStore();
		const settings = await store.get<Settings>("settings");
		if (!settings) {
			return createDefaultSettingsObject();
		}

		let needsUpdate = normalizeSettingsArrays(settings);

		// Migration: Ensure existing users have deviceId for free tier tracking
		const existingUserGoal = normalizeUserGoalCategory(
			settings.userGoalCategory,
		);
		const shouldMigrateUserGoal = existingUserGoal === null;
		if (shouldMigrateUserGoal) {
			settings.userGoalCategory = readLegacyUserGoalCategory();
			needsUpdate = true;
		} else {
			settings.userGoalCategory = existingUserGoal;
		}
		if (!settings.deviceId) {
			settings.deviceId = crypto.randomUUID();
			needsUpdate = true;
		}

		// Temporary one-time migration: force restart notifications off for all
		// existing users until the stall detector is more reliable. Users can
		// still manually opt back in afterward; the marker prevents re-overriding.
		if (!(settings as any).restartNotificationsDefaultedOff) {
			settings.showRestartNotifications = false;
			(settings as any).restartNotificationsDefaultedOff = true;
			needsUpdate = true;
		}

		// One-time migration (V2 — supersedes V1): flip the CoreAudio Process
		// Tap toggle OFF for every existing install, keeping SCK as the System
		// Audio backend. V1 (run a few days earlier) had flipped it ON by
		// default, but the Process Tap can't capture audio rendered through a
		// VoiceProcessing AudioUnit — Zoom/Meet/Teams all use one for echo
		// cancellation — so the tap silently captured zeroed buffers on every
		// meeting. Users who explicitly want the tap (e.g. to dodge SCK's
		// sleep/wake display-enumeration bug) can re-enable it in Settings.
		// Reported on 2026-04-24 after v2.4.46 calls kept dropping
		// other participants.
		if (!(settings as any).coreaudioTapMigrationV2) {
			settings.experimentalCoreaudioSystemAudio = false;
			(settings as any).coreaudioTapMigrationV2 = true;
			needsUpdate = true;
		}

		// One-time migration (V3 — supersedes V2): flip CoreAudio Process Tap
		// back ON. The toggle was removed from the UI and the runtime falls back
		// through normal backend selection if tap initialization fails (#5236).
		if (!(settings as any).coreaudioTapMigrationV3) {
			settings.experimentalCoreaudioSystemAudio = true;
			(settings as any).coreaudioTapMigrationV3 = true;
			needsUpdate = true;
		}

		// One-time migration: default filterMusic to ON — transcribing music
		// as speech is noise, users can turn it off in advanced settings (#5236).
		if (!(settings as any).filterMusicDefaultedOn) {
			settings.filterMusic = true;
			(settings as any).filterMusicDefaultedOn = true;
			needsUpdate = true;
		}

		if (settings.meetingLiveTranscriptionEnabled === undefined) {
			settings.meetingLiveTranscriptionEnabled = true;
			needsUpdate = true;
		}
		if (!settings.meetingLiveTranscriptionProvider) {
			settings.meetingLiveTranscriptionProvider = "selected-engine";
			needsUpdate = true;
		}
		if (settings.appendTypedTextToMeetingNote === undefined) {
			settings.appendTypedTextToMeetingNote = true;
			needsUpdate = true;
		}

		// NOTE: audioCaptureMode is intentionally NOT backfilled for existing
		// installs. Their stored settings have no value for it, so the engine's
		// serde default ("always") and the UI's `?? "always"` fallback keep them on
		// continuous capture — without writing anything to their store. Only brand-new
		// installs default to "meetings-only" (via createDefaultSettingsObject, which
		// get() returns directly when there are no stored settings).

		if ((settings as any).user) {
			// This build has no Screenpipe account services. Drop the cached
			// account metadata so it cannot revive account or billing UI.
			(settings as any).user = null;
			needsUpdate = true;
		}
		if (settings.aiPresets?.some((p: any) => p.provider === "screenpipe-cloud")) {
			// Remove only the retired hosted preset. Never substitute an endpoint
			// or alter user-configured Runtime and third-party providers.
			settings.aiPresets = settings.aiPresets.filter(
				(p: any) => p.provider !== "screenpipe-cloud",
			);
			needsUpdate = true;
		}
		if (settings.audioTranscriptionEngine === "screenpipe-cloud") {
			settings.audioTranscriptionEngine = DEFAULT_AUDIO_ENGINE;
			settings.meetingLiveTranscriptionProvider = "selected-engine";
			needsUpdate = true;
		}
		if ((settings as any).enterpriseManagedSettings) {
			// Stale enterprise policy blob from a previous install: drop it.
			delete (settings as any).enterpriseManagedSettings;
			needsUpdate = true;
		}

		// Migration: Add chat history for existing users
		if (!settings.chatHistory) {
			settings.chatHistory = {
				conversations: [],
				activeConversationId: null,
				historyEnabled: true,
			};
			needsUpdate = true;
		}

		// Migration: Fill empty showChatShortcut with platform default
		if (!settings.showChatShortcut || settings.showChatShortcut.trim() === "") {
			const p = platform();
			settings.showChatShortcut = p === "windows" ? "Alt+L" : "Control+Super+L";
			needsUpdate = true;
		}

		// Migration: Fill empty audio shortcuts with platform defaults
		if (!settings.startAudioShortcut || settings.startAudioShortcut.trim() === "") {
			const p = platform();
			settings.startAudioShortcut = p === "windows" ? "Alt+Shift+A" : "Control+Super+A";
			needsUpdate = true;
		}
		if (!settings.stopAudioShortcut || settings.stopAudioShortcut.trim() === "") {
			const p = platform();
			settings.stopAudioShortcut = p === "windows" ? "Alt+Shift+Z" : "Control+Super+Z";
			needsUpdate = true;
		}

		// Always override platform with runtime detection — never trust persisted value.
		// Platform can be "unknown" if it was saved during SSR or before Tauri was ready.
		try {
			const detectedPlatform = platform();
			if (settings.platform !== detectedPlatform) {
				settings.platform = detectedPlatform;
				needsUpdate = true;
			}
		} catch {
			// platform() unavailable (SSR/tests) — keep existing value
		}

		// Mark pro migration as done so the old migration doesn't re-trigger
		if (!(settings as any)._proCloudMigrationDone) {
			(settings as any)._proCloudMigrationDone = true;
			needsUpdate = true;
		}

		// Migration: migrate removed transcription engines to qwen3-asr
		// (parakeet/parakeet-mlx were cloud-unreachable, deepgram and
		// openai-compatible were cloud engines removed in the local-only fork).
		// User-selected whisper variants are preserved — they still work.
		if (!(settings as any)._parakeetDefaultMigrationDone) {
			const engine = settings.audioTranscriptionEngine;
			const isWhisperVariant = engine?.includes("whisper");
			if (
				!isWhisperVariant &&
				engine &&
				engine !== "qwen3-asr" &&
				engine !== "disabled"
			) {
				settings.audioTranscriptionEngine = "qwen3-asr";
				needsUpdate = true;
			}
			(settings as any)._parakeetDefaultMigrationDone = true;
			needsUpdate = true;
		}

		// Migration: backfill disabledShortcuts for installs that predate the
		// field. Several call sites assume it's always an array (`.includes(...)`)
		// and crash with "Cannot read properties of undefined" when it's missing.
			if (!Array.isArray(settings.disabledShortcuts)) {
				settings.disabledShortcuts = [];
				needsUpdate = true;
			}

		// Save migrations if needed
		if (needsUpdate) {
			await persistSettings(store, settings);
			await saveAndEncrypt(store);
		}

		// Remove browser-only values only after the durable app setting exists.
		// This makes the migration one-shot without risking data loss on a failed
		// settings write.
		if (shouldMigrateUserGoal) clearLegacyUserGoalCategory();

		return settings;
	};

	const set = (value: Partial<Settings>) =>
		enqueueSettingsStoreWrite(async () => {
			assertValidAiPresetUpdate(value);
			const store = await getStore();
			const current = await get();
			const newSettings = { ...current, ...value } as Settings;
			await persistSettings(store, newSettings);
			await saveAndEncrypt(store);
		});

	const reset = () =>
		enqueueSettingsStoreWrite(async () => {
			const store = await getStore();
			const defaults = createDefaultSettingsObject();
			await store.set("settings", defaults);
			await saveAndEncrypt(store);
		});

	const resetSetting = async <K extends keyof Settings>(key: K) => {
		const current = await get();
		const defaultValue = createDefaultSettingsObject()[key];
		await set({ [key]: defaultValue } as Partial<Settings>);
	};

	const listen = (callback: (settings: Settings) => void) => {
		return getStore().then((store) => {
			let seq = 0;
			return store.onKeyChange("settings", async (newValue: Settings | null | undefined) => {
				const mySeq = ++seq;
				const next = newValue || createDefaultSettingsObject();
				normalizeSettingsArrays(next);
				if (mySeq === seq) callback(next);
			});
		});
	};

	return {
		get,
		set,
		reset,
		resetSetting,
		listen,
	};
}

const settingsStore = createSettingsStore();

// Context for React
interface SettingsContextType {
	settings: Settings;
	updateSettings: (updates: Partial<Settings>) => Promise<void>;
	resetSettings: () => Promise<void>;
	resetSetting: <K extends keyof Settings>(key: K) => Promise<void>;
	reloadStore: () => Promise<void>;
	getDataDir: () => Promise<string>;
	isSettingsLoaded: boolean;
	loadingError: string | null;
}

const SettingsContext = createContext<SettingsContextType | undefined>(undefined);

export const SettingsProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
	const [settings, setSettings] = useState<Settings>(createDefaultSettingsObject());
	const [isSettingsLoaded, setIsSettingsLoaded] = useState(false);
	const [loadingError, setLoadingError] = useState<string | null>(null);

	// Load settings on mount
	useEffect(() => {
		const loadSettings = async () => {
			try {
				const loadedSettings = await settingsStore.get();
				setSettings(loadedSettings);
				setIsSettingsLoaded(true);
				setLoadingError(null);

				// Configure the API module — single source of truth for port + auth.
				// `apiKey` is intentionally NOT passed: `ensureInitialized` in
				// lib/api.ts loads the canonical key from the server via IPC
				// (`get_local_api_config`). settings.apiKey is a user preference
				// fed to the server's auth resolver; the server then exposes the
				// resolved key via that IPC. Passing it here would race with the
				// IPC and overwrite a good key with `null` for the majority of
				// users (who never set a custom api key) — which silently breaks
				// every WebSocket auth path.
				const { configureApi, refreshApiConfig } = await import("@/lib/api");
				configureApi({
					port: loadedSettings.port ?? 3030,
					authEnabled: loadedSettings.apiAuth ?? true,
				});
				// The running backend is the source of truth for effective overrides
				// such as SCREENPIPE_PORT. Refresh after applying the persisted
				// fallback so every window talks to the server that actually started.
				await refreshApiConfig();

				// Hydrate Rust's owned-browser runtime cache from persisted settings.
				// This prevents the cookie-access prompt from reappearing after restart.
				await commands
					.setBrowserCookieAccessState(
						loadedSettings.browserCookieAccessGranted === true,
						loadedSettings.browserCookieAccessGranted === false,
					)
					.catch(() => {});
			} catch (error) {
				console.error("Failed to load settings:", error);
				setLoadingError(error instanceof Error ? error.message : "Unknown error");
				setIsSettingsLoaded(true);
			}
		};

		loadSettings();

		// Listen for changes
		const unsubscribe = settingsStore.listen((newSettings) => {
			setSettings(newSettings);
		});

		return () => {
			unsubscribe.then((unsub) => unsub());
		};
	}, []);

	const settingsRef = useRef(settings);
	settingsRef.current = settings;
	const settingsUpdateGenerationRef = useRef(0);

	useEffect(() => {
		applyFontSize(settings.fontSize);
	}, [settings.fontSize]);

	const updateSettings = async (updates: Partial<Settings>) => {
		assertValidAiPresetUpdate(updates);
		const updateGeneration = ++settingsUpdateGenerationRef.current;
		const settingsBeforeUpdate = settingsRef.current;

		// Controlled switches and checkboxes must reflect the click immediately.
		// Waiting for the asynchronous store listener makes React render the old
		// value again, so the first click appears to undo itself. Persistence stays
		// authoritative: a failed latest write is rolled back below.
		setSettings((current) => ({ ...current, ...updates }) as Settings);

		try {
			await settingsStore.set(updates);
		} catch (error) {
			// Do not let an older failed write overwrite a newer optimistic click.
			// The queued newer write (and its store event) owns reconciliation.
			if (settingsUpdateGenerationRef.current === updateGeneration) {
				try {
					setSettings(await settingsStore.get());
				} catch {
					setSettings(settingsBeforeUpdate);
				}
			}
			throw error;
		}

		// Only update the port in the API module immediately — auth changes
		// (apiAuth / apiKey) must NOT be applied until after the server restarts.
		// Calling configureApi({ authEnabled: false }) before restart clears the
		// auth cookie, causing every frontend WebSocket to reconnect without a
		// token and flood the logs with 403 rejections (the server still requires
		// auth until it restarts with the new setting).
		if ("port" in updates) {
			const { configureApi } = await import("@/lib/api");
			const merged = { ...settings, ...updates };
			configureApi({ port: merged.port ?? 3030 });
		}
	};

	const resetSettings = async () => {
		await settingsStore.reset();
		// Settings will be updated via the listener
	};

	const resetSetting = async <K extends keyof Settings>(key: K) => {
		await settingsStore.resetSetting(key);
		// Settings will be updated via the listener
	};

	const reloadStore = async () => {
		const freshSettings = await settingsStore.get();
		setSettings(freshSettings);
	};

	const getDataDir = async () => {
		const homeDirPath = await homeDir();

		if (
			settings.dataDir !== "default" &&
			settings.dataDir &&
			settings.dataDir !== ""
		)
			return settings.dataDir;

		return `${homeDirPath}/.screenpipe`;
	};

	const value: SettingsContextType = {
		settings,
		updateSettings,
		resetSettings,
		resetSetting,
		reloadStore,
		getDataDir,
		isSettingsLoaded,
		loadingError,
	};

	return (
		<SettingsContext.Provider value={value}>
			{children}
		</SettingsContext.Provider>
	);
};

export function useSettings(): SettingsContextType {
	const context = useContext(SettingsContext);
	if (context === undefined) {
		throw new Error("useSettings must be used within a SettingsProvider");
	}
	return context;
}
