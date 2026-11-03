import { useEffect, useRef, useState } from 'react';
import {
  ArrowUpRight,
  AudioLines,
  CircleAlert,
  PanelLeftOpen,
  SlidersHorizontal,
  X,
} from 'lucide-react';
import { isTauri, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { CustomTitleBar } from './layouts/CustomTitleBar';
import { Sidebar } from './layouts/Sidebar';
import { Modal } from './components/Modal';
import { ChatComposer, type UploadProgress } from './features/chat/ChatComposer';
import { FileConflict, type ConflictInfo } from './features/system-actions/FileConflict';
import { FileTargetResolver } from './features/system-actions/FileTargetResolver';
import { ToolConfirmation } from './features/system-actions/ToolConfirmation';
import type { ApprovalView } from './features/system-actions/TargetDetails';

type WakeRuntimeState = 'disabled' | 'starting' | 'listening' | 'triggered' | 'paused' | 'error';
type WakeRuntimeStatus = {
  state: WakeRuntimeState;
  enabled: boolean;
  error?: string;
};
type WakeSetupStatus = {
  completedSamples: number;
  totalSamples: number;
  settings: {
    enabled: boolean;
    setupCompleted: boolean;
    referenceExists: boolean;
    defaultAvailable: boolean;
    modelSource: 'default' | 'personal' | null;
  };
};
import { MessageList } from './features/chat/MessageList';
import { TrashModal } from './features/trash/TrashModal';
import { AuthGate } from './features/auth/AuthGate';
import { SettingsModal } from './features/settings/SettingsModal';
import { WakeWordSetupModal } from './features/voice/WakeWordSetupModal';
import {
  classifyVoiceCommand,
  normalizeAppArguments,
  localPolicy,
  systemAdapter,
  toolRequest,
  toolLabel,
  confirmationPrompt,
  confirmationView,
} from './features/system-actions/adapters/systemAdapter';
import { conversationApi } from './api/conversations.api';
import { messagesApi } from './api/messages.api';
import { attachmentsApi } from './api/attachments.api';
import { settingsApi } from './api/settings.api';
import { agentApi } from './api/agent.api';
import { speakAssistant, stopSpeech } from './api/speech.api';
import { recordVoiceLog } from './api/voice.api';
import { normalizeVoiceLogErrorCode } from './api/voice-log-errors';
import { apiErrorMessage, isCancelled } from './api/client';
import { useConversations } from './hooks/useConversations';
import { useMessages } from './hooks/useMessages';
import type {
  Message,
  Conversation,
  SystemActionRequest,
  SystemActionStatus,
  ToolCall,
  User,
} from './types';
import './styles/globals.css';
import './styles/chat-experience.css';
import './styles/motion.css';

type Action = { status: SystemActionStatus; message: string };
type WindowChoice = { targetId: string; appName: string; title: string };
type CaptureMonitor = {
  monitorId: string;
  name: string;
  width: number;
  height: number;
  scaleFactor: number;
};
type PendingAction = {
  fileIntent?: Record<string, unknown>;
  fileSearchScope?: boolean;
  fileConflict?: ConflictInfo;
  nativeApproval?: { pendingId: string; description: string; view?: ApprovalView };
  fileProblem?: string;
  captureChoices?: CaptureMonitor[];
  windowChoices?: WindowChoice[];
  request?: SystemActionRequest;
  call?: ToolCall;
  voice: boolean;
  voiceRequestId?: string;
  permission?: { denied: boolean; requiresConfirmation: boolean };
};
type VoiceChoice = { id: string; label: string };
const reportVoice = (
  item: PendingAction,
  status: string,
  message: string,
  choices?: VoiceChoice[],
) =>
  item.voiceRequestId
    ? invoke('report_voice_tool_result', {
        requestId: item.voiceRequestId,
        status,
        message,
        choices: choices ?? [],
      })
    : Promise.resolve();
function AceApp({ user, logout }: { user: User; logout: () => Promise<void> }) {
  const conversations = useConversations();
  const [active, setActive] = useState<string | null>(null);
  const [draftVersion, setDraftVersion] = useState(0);
  const messages = useMessages(active);
  const activeRef = useRef(active);
  activeRef.current = active;
  const appendRef = useRef(messages.append);
  appendRef.current = messages.append;
  const [optimistic, setOptimistic] = useState<Message | null>(null);
  const [animateIds, setAnimateIds] = useState<Set<string>>(() => new Set());
  const [typingReplyIds, setTypingReplyIds] = useState<Set<string>>(() => new Set());
  const sendAbort = useRef<AbortController | null>(null);
  const deletedConversations = useRef(new Set<string>());
  const reconcileRequests = useRef(new Map<AbortController, string>());
  const animateReply = (message: Message | null) => {
    if (!message) return;
    setAnimateIds((previous) => new Set([...previous, message.id]));
    setTypingReplyIds((previous) => new Set([...previous, message.id]));
  };
  const finishReply = (id: string) =>
    setTypingReplyIds((previous) => {
      if (!previous.has(id)) return previous;
      const next = new Set(previous);
      next.delete(id);
      return next;
    });
  const stopSend = () => {
    sendAbort.current?.abort();
    stopSpeech();
    void systemAdapter.cancelPendingCapture();
  };

  const [collapsed, setCollapsed] = useState(false);
  const sidebarPanel = useRef<HTMLDivElement>(null);
  const expandSidebar = useRef<HTMLButtonElement>(null);
  const previousCollapsed = useRef(false);
  useEffect(() => {
    if (previousCollapsed.current !== collapsed) {
      if (collapsed) expandSidebar.current?.focus();
      else
        sidebarPanel.current
          ?.querySelector<HTMLButtonElement>('[aria-label="사이드바 접기"]')
          ?.focus();
      previousCollapsed.current = collapsed;
    }
  }, [collapsed]);
  const [modal, setModal] = useState<'settings' | 'trash' | null>(null);
  const [rename, setRename] = useState<Conversation | null>(null);
  const [name, setName] = useState('');
  const [theme, setTheme] = useState(() => localStorage.getItem('ace-theme') || 'light');
  const [wake, setWake] = useState(false);
  const [wakeState, setWakeState] = useState<WakeRuntimeState>('disabled');
  const [wakeSetupCompleted, setWakeSetupCompleted] = useState(false);
  const [wakeReferenceExists, setWakeReferenceExists] = useState(false);
  const [wakeSetupOpen, setWakeSetupOpen] = useState(false);
  const [wakeDefaultAvailable, setWakeDefaultAvailable] = useState(false);
  const [wakeModelSource, setWakeModelSource] = useState<'default' | 'personal' | null>(null);
  const [action, setAction] = useState<Action | null>(null);
  useEffect(() => {
    if (action?.status !== 'error') return;
    const timer = setTimeout(
      () => setAction((current) => (current === action ? null : current)),
      5000,
    );
    return () => clearTimeout(timer);
  }, [action]);
  const [pending, setPending] = useState<PendingAction[]>([]);
  const [searchingConversation, setSearchingConversation] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [mutating, setMutating] = useState(false);
  const [notice, setNotice] = useState('');
  const [permissionTarget, setPermissionTarget] = useState<string>();
  const [lastCapture, setLastCapture] = useState<{
    resourceId: string;
    displayName: string;
  } | null>(null);
  const [captureSaveName, setCaptureSaveName] = useState('캡처.png');
  const [fileSearchResults, setFileSearchResults] = useState<
    | {
        resourceId: string;
        displayName: string;
        relativePath?: string;
        executable?: boolean;
        directory?: string;
      }[]
    | null
  >(null);
  const [searchSummary, setSearchSummary] = useState('');
  const [searchFolders, setSearchFolders] = useState(false);
  const [captureSaving, setCaptureSaving] = useState(false);
  const captureSaveLock = useRef(false);
  useEffect(() => {
    const id = lastCapture?.resourceId;
    return () => {
      if (id && isTauri())
        void invoke('release_capture_file', { resourceId: id }).catch(() => undefined);
    };
  }, [lastCapture?.resourceId]);
  const [clipboardPreview, setClipboardPreview] = useState<string | null>(null);
  const [aiReady, setAiReady] = useState<boolean | null>(null);
  const lock = useRef(false);
  const mutationLock = useRef(false);
  const executionLock = useRef(false);
  const fileChoiceLock = useRef(false);
  const approvalLock = useRef(false);
  const mounted = useRef(true);
  const voiceLock = useRef(false);
  const voiceChoiceHandler = useRef<(id: string, choice: string) => void>(() => {});
  const voiceSearchChoices = useRef(
    new Map<string, { id: string; label: string; request: SystemActionRequest }[]>(),
  );
  const cancelledVoice = useRef(new Set<string>());
  useEffect(() => {
    mounted.current = true;
    const controller = new AbortController();
    agentApi
      .health(controller.signal)
      .then((data) => setAiReady(data.ai !== 'not_configured'))
      .catch((cause) => {
        if (!isCancelled(cause)) setNotice(apiErrorMessage(cause));
      });
    return () => {
      mounted.current = false;
      stopSpeech();
      sendAbort.current?.abort();
      controller.abort();
    };
  }, []);
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem('ace-theme', theme);
  }, [theme]);
  useEffect(() => {
    let secondFrame = 0;
    const firstFrame = requestAnimationFrame(() => {
      secondFrame = requestAnimationFrame(() => {
        document.documentElement.dataset.motionReady = 'true';
      });
    });
    return () => {
      cancelAnimationFrame(firstFrame);
      cancelAnimationFrame(secondFrame);
      delete document.documentElement.dataset.motionReady;
    };
  }, []);
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(''), 6500);
    return () => clearTimeout(timer);
  }, [notice]);
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const stops: (() => void)[] = [];
    const register = async <T,>(event: string, callback: (payload: T) => void) => {
      const unlisten = await listen<T>(event, ({ payload }) => callback(payload));
      if (disposed) unlisten();
      else stops.push(unlisten);
    };
    void register<string>('ace-voice-command', (payload) => {
      if (typeof payload === 'string' && payload.length <= 200) void runVoice(payload.trim());
    });
    void register<string>('ace-speech-request', (text) => {
      if (typeof text === 'string' && text.length <= 30000)
        void speakAssistant(text).catch(() =>
          setNotice('음성 재생에 실패했습니다. 답변은 화면에서 확인해 주세요.'),
        );
    });
    void register('ace-speech-stop', () => stopSpeech());
    void register<{ tool: string; arguments: Record<string, unknown>; requestId: string }>(
      'ace-voice-tool-call',
      (payload) => {
        if (
          !payload ||
          typeof payload.tool !== 'string' ||
          !payload.arguments ||
          typeof payload.requestId !== 'string'
        )
          return;
        void reportVoice(
          { voice: true, voiceRequestId: payload.requestId },
          'EXECUTING',
          '요청을 확인하고 있어요.',
        );
        const riskLevel = localPolicy(payload.tool);
        void settingsApi
          .permissions()
          .then((preferences) => {
            if (cancelledVoice.current.has(payload.requestId) || !mounted.current) return;
            const permission = preferences.find((value) => value.toolName === payload.tool);
            setPending((previous) => [
              ...previous,
              {
                request: {
                  contractVersion: '1.1',
                  commandType: payload.tool,
                  requestId: payload.requestId,
                  arguments: payload.arguments,
                  riskLevel,
                  label: toolLabel(payload.tool, payload.arguments),
                },
                voice: true,
                voiceRequestId: payload.requestId,
                permission,
              },
            ]);
          })
          .catch(
            () =>
              void reportVoice(
                { voice: true, voiceRequestId: payload.requestId },
                'ERROR',
                '권한 설정을 확인하지 못했습니다. 다시 시도해 주세요.',
              ),
          );
      },
    );
    void register<{ requestId: string; choiceId: string }>('ace-voice-choice', (payload) =>
      voiceChoiceHandler.current(payload.requestId, payload.choiceId),
    );
    void register<string>('ace-voice-request-cancelled', (id) => {
      cancelledVoice.current.add(id);
      if (cancelledVoice.current.size > 100)
        cancelledVoice.current.delete(cancelledVoice.current.values().next().value!);
      voiceSearchChoices.current.delete(id);
      setPending((previous) => previous.filter((item) => item.voiceRequestId !== id));
    });
    void register<boolean>('ace-wake-word-changed', (enabled) => {
      if (typeof enabled === 'boolean') setWake(enabled);
    });
    const applyWakeStatus = (status: WakeRuntimeStatus) => {
      setWake(status.enabled);
      setWakeState(status.state);
      if (status.state === 'error' && status.error) setNotice(status.error);
    };
    void register<WakeRuntimeStatus>('ace-wake-word-status', applyWakeStatus);
    void register('ace-open-settings', () => {
      setModal('settings');
      void invoke('take_pending_settings_request');
    });
    void invoke<WakeSetupStatus>('get_wake_word_setup_status')
      .then((setup) => {
        if (disposed) return undefined;
        setWake(setup.settings.enabled);
        setWakeSetupCompleted(setup.settings.setupCompleted);
        setWakeReferenceExists(setup.settings.referenceExists);
        setWakeDefaultAvailable(setup.settings.defaultAvailable);
        setWakeModelSource(setup.settings.modelSource);
        return invoke<WakeRuntimeStatus>('get_wake_word_status');
      })
      .then((status) => {
        if (!disposed && status) applyWakeStatus(status);
      })
      .catch((cause) => setNotice(apiErrorMessage(cause)));
    void invoke<boolean>('take_pending_settings_request')
      .then((pending) => {
        if (!disposed && pending) setModal('settings');
      })
      .catch((cause) => setNotice(apiErrorMessage(cause)));
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
      void invoke('hide_voice_overlay');
    };
  }, []);
  const update = async (
    conversation: Conversation,
    type: 'pin' | 'delete' | 'rename',
    title?: string,
  ) => {
    if (mutationLock.current) return;
    mutationLock.current = true;
    setMutating(true);
    try {
      if (type === 'delete') {
        await conversationApi.remove(conversation.id);
        deletedConversations.current.add(conversation.id);
        for (const [controller, id] of reconcileRequests.current)
          if (id === conversation.id) controller.abort();
        conversations.remove(conversation.id);
        if (activeRef.current === conversation.id) {
          activeRef.current = null;
          stopSend();
          messages.clear();
          setActive(null);
          setOptimistic(null);
          setAnimateIds(new Set());
          setTypingReplyIds(new Set());
          setPending((previous) => previous.filter((item) => item.voice));
          setAction(null);
          setDraftVersion((version) => version + 1);
        }
      } else
        conversations.upsert(
          await conversationApi.update(
            conversation.id,
            type === 'pin' ? { isPinned: !conversation.isPinned } : { title },
          ),
        );
      setRename(null);
    } catch (cause) {
      setNotice(apiErrorMessage(cause));
    } finally {
      mutationLock.current = false;
      if (mounted.current) setMutating(false);
    }
  };
  const startNewChat = () => {
    if (busy) return;
    setActive(null);
    setOptimistic(null);
    setDraftVersion((version) => version + 1);
    setNotice('');
  };
  const reconcile = async (id: string) => {
    if (deletedConversations.current.has(id)) return [];
    const controller = new AbortController();
    reconcileRequests.current.set(controller, id);
    try {
      const page = await messagesApi.list(id, null, controller.signal);
      if (
        !controller.signal.aborted &&
        !deletedConversations.current.has(id) &&
        mounted.current &&
        id === activeRef.current
      )
        page.items.slice().reverse().forEach(appendRef.current);
      return page.items;
    } finally {
      reconcileRequests.current.delete(controller);
    }
  };
  const send = async (
    text: string,
    files: File[] = [],
    onProgress: (progress: UploadProgress) => void = () => undefined,
  ): Promise<boolean> => {
    if (lock.current) return false;
    if (text.length > 10000) {
      setNotice('메시지는 10,000자 이내로 입력해 주세요.');
      return false;
    }
    lock.current = true;
    const abort = new AbortController();
    sendAbort.current = abort;
    setSending(true);
    let id = active;
    let saved: Message | null = null;
    const uploaded: string[] = [];
    setOptimistic({
      id: 'pending-' + crypto.randomUUID(),
      conversationId: id || 'pending',
      role: 'USER',
      attachments: [],
      content: text,
      createdAt: new Date().toISOString(),
    });
    try {
      if (!id) {
        const conversation = await conversationApi.create();
        id = conversation.id;
        conversations.upsert(conversation);
        setActive(id);
        setOptimistic((previous) => (previous ? { ...previous, conversationId: id! } : null));
      }
      for (let index = 0; index < files.length; index += 1) {
        onProgress({ index, status: 'uploading', progress: 0 });
        const attachment = await attachmentsApi.upload(id, files[index], (progress) =>
          onProgress({ index, status: 'uploading', progress }),
        );
        uploaded.push(attachment.id);
        onProgress({ index, status: 'uploaded', progress: 100 });
      }
      // Persist the user bubble independently. Stopping AI must not undo the sent message.
      saved = await messagesApi.create(id, text, uploaded, abort.signal);
      if (deletedConversations.current.has(id) || abort.signal.aborted) return true;
      setOptimistic(saved);
      appendRef.current(saved);
      const updated = await conversationApi.get(id, abort.signal);
      if (deletedConversations.current.has(id) || abort.signal.aborted) return true;
      conversations.upsert(updated);
      if (abort.signal.aborted) return true;
      const configured = aiReady ?? (await agentApi.health()).ai !== 'not_configured';
      setAiReady(configured);
      if (configured && text.trim()) {
        const turn = await agentApi.turn(id, text, [], saved.id, abort.signal);
        if (abort.signal.aborted || !mounted.current) return true;
        animateReply(turn.message);
        if (turn.message) {
          appendRef.current(turn.message);
          void speakAssistant(turn.message.content).catch(() =>
            setNotice('답변은 저장했지만 음성 재생에 실패했습니다.'),
          );
        }
        await reconcile(id);
        if (abort.signal.aborted) return true;
        setPending((previous) => [
          ...previous,
          ...turn.toolCalls.map((call) => ({ call, voice: false })),
        ]);
        const updated = await conversationApi.get(id, abort.signal);
        if (!deletedConversations.current.has(id) && !abort.signal.aborted)
          conversations.upsert(updated);
      } else if (!configured)
        setNotice('메시지를 저장했습니다. AI 서버 연결 후 답변을 받을 수 있습니다.');
      return true;
    } catch (cause) {
      if (id && deletedConversations.current.has(id)) return true;
      if (abort.signal.aborted && saved) return true;
      if (mounted.current) setNotice(apiErrorMessage(cause));
      if (saved) return true;
      setOptimistic(null);
      await Promise.allSettled(uploaded.map((attachmentId) => attachmentsApi.remove(attachmentId)));
      return false;
    } finally {
      lock.current = false;
      sendAbort.current = null;
      if (mounted.current) setSending(false);
    }
  };
  const logVoice = async (
    request: SystemActionRequest,
    status: 'SUCCESS' | 'FAILED' | 'CANCELLED',
    duration: number,
    errorCode?: string,
  ) => {
    try {
      const normalizedErrorCode = normalizeVoiceLogErrorCode(errorCode);
      await recordVoiceLog({
        commandType: request.commandType,
        status,
        duration,
        ...(normalizedErrorCode ? { errorCode: normalizedErrorCode } : {}),
      });
    } catch (cause) {
      if (mounted.current) setNotice(`음성 실행 로그 저장 실패: ${apiErrorMessage(cause)}`);
    }
  };
  const execute = async (
    item: PendingAction,
    approved: boolean,
    approvalFailure?: {
      success: false;
      errorCode: string;
      message: string;
      data?: Record<string, unknown>;
    },
  ) => {
    if (executionLock.current) return;
    if (item.voiceRequestId && cancelledVoice.current.has(item.voiceRequestId)) return;
    executionLock.current = true;
    const start = performance.now();
    const request = item.request || (item.call ? toolRequest(item.call) : null);
    const showSearchProgress =
      approved &&
      !approvalFailure &&
      !!item.call &&
      !item.voiceRequestId &&
      !!request &&
      ['file.search', 'folder.search'].includes(request.commandType);
    if (showSearchProgress) setSearchingConversation(active);
    if (request?.commandType === 'clipboard.read') setClipboardPreview(null);
    if (request) request.label = toolLabel(request.commandType, request.arguments);
    let waitingForWindow = false;
    let reply: { status: string; message: string; choices?: VoiceChoice[] } | null = null;
    if (!item.voiceRequestId) setAction({ status: 'pending', message: '명령 처리 중…' });
    try {
      await reportVoice(item, 'EXECUTING', '실행 중이에요.');
      if (item.voiceRequestId && cancelledVoice.current.has(item.voiceRequestId)) return;
      if (item.call?.executionLocation === 'CLOUD') {
        if (!approved) {
          setAction({ status: 'error', message: '도구 실행을 취소했습니다.' });
          return;
        }
        const turn = await agentApi.cloud(item.call, true);
        animateReply(turn.message);
        if (turn.message)
          void speakAssistant(turn.message.content).catch(() =>
            setNotice('음성 재생에 실패했습니다.'),
          );
        await reconcile(turn.conversationId);
        setPending((previous) => [
          ...previous,
          ...turn.toolCalls.map((call) => ({ call, voice: false })),
        ]);
        setAction({ status: 'success', message: '도구 실행을 완료했습니다.' });
      } else if (request) {
        if (['file.search', 'folder.search'].includes(request.commandType))
          setFileSearchResults(null);
        item.request = request;
        let deniedByPolicy = !!(item.call?.denied || item.permission?.denied);
        if (!request.executionTicket && isTauri() && request.commandType !== 'system.status') {
          const registration = await agentApi.registerExecution({
            ...request,
            arguments: normalizeAppArguments(request.commandType, request.arguments),
          });
          request.executionId = registration.executionId;
          request.requestId = registration.requestId;
          request.executionTicket = registration.ticket;
          if (registration.denied) {
            approved = false;
            deniedByPolicy = true;
          }
        }
        request.executionId ||= crypto.randomUUID();
        request.attemptId ||= crypto.randomUUID();
        await agentApi.executionEvent(request, 'REQUESTED');
        const result =
          approvalFailure ??
          (approved
            ? await systemAdapter.execute(request, true)
            : deniedByPolicy
              ? {
                  success: false,
                  errorCode: 'TOOL_PERMISSION_DENIED',
                  message: '권한 설정에서 차단되었습니다.',
                }
              : { success: false, errorCode: 'CANCELLED' });
        if (deniedByPolicy) {
          setPermissionTarget(request.commandType);
          setNotice('권한 설정에서 차단되었습니다.');
        }
        if (item.voiceRequestId && cancelledVoice.current.has(item.voiceRequestId)) return;
        if (result.errorCode === 'NATIVE_APPROVAL_REQUIRED') {
          waitingForWindow = true;
          const nativeApproval = result.data as {
            pendingId: string;
            description: string;
            view?: ApprovalView;
          };
          setPending((previous) =>
            previous.map((candidate) =>
              candidate === item
                ? {
                    ...item,
                    request,
                    nativeApproval,
                    fileIntent: undefined,
                    windowChoices: undefined,
                  }
                : candidate,
            ),
          );
          setAction(null);
          reply = {
            status: 'WAITING_CONFIRMATION',
            message: `${toolLabel(request.commandType)}\n${nativeApproval.description}`,
            choices: [
              { id: 'yes', label: nativeApproval.view?.action || '실행' },
              { id: 'no', label: '취소' },
            ],
          };
          return;
        }
        if (result.errorCode === 'FILE_SEARCH_SCOPE_REQUIRED') {
          waitingForWindow = true;
          setPending((previous) =>
            previous.map((candidate) =>
              candidate === item ? { ...item, request, fileSearchScope: true } : candidate,
            ),
          );
          setAction(null);
          reply = {
            status: 'WAITING_CONFIRMATION',
            message: '메인 창에서 파일을 찾을 위치를 선택해 주세요.',
          };
          return;
        }
        if (result.errorCode === 'FILE_TARGET_SELECTION_REQUIRED') {
          waitingForWindow = true;
          setPending((previous) =>
            previous.map((candidate) =>
              candidate === item ? { ...item, request, fileIntent: result.data } : candidate,
            ),
          );
          setAction(null);
          reply = {
            status: 'WAITING_CONFIRMATION',
            message: '메인 창에서 파일과 대상 위치를 확인해 주세요.',
          };
          return;
        }
        if (
          result.errorCode === 'FILE_OVERWRITE_CONFIRMATION_REQUIRED' ||
          result.errorCode === 'FILE_CONFIRMATION_STALE' ||
          (result.errorCode === 'SAFE_OVERWRITE_UNAVAILABLE' && item.fileConflict)
        ) {
          const conflict = (result.data?.conflict as ConflictInfo | undefined) ?? item.fileConflict;
          if (conflict) {
            waitingForWindow = true;
            const next = {
              ...item,
              request,
              fileConflict: conflict,
              fileProblem:
                result.errorCode === 'FILE_OVERWRITE_CONFIRMATION_REQUIRED'
                  ? undefined
                  : result.message,
            };
            setPending((previous) =>
              previous.map((candidate) => (candidate === item ? next : candidate)),
            );
            if (!item.voiceRequestId) setAction(null);
            reply = {
              status: 'WAITING_CONFIRMATION',
              message: '같은 이름의 파일이 있습니다. 원본과 대상을 확인해 주세요.',
              choices: [
                { id: 'overwrite', label: '덮어쓰기' },
                { id: 'cancel', label: '취소' },
              ],
            };
            return;
          }
        }
        if (result.errorCode === 'CAPTURE_SELECTION_REQUIRED') {
          const monitors = Array.isArray(result.data?.monitors)
            ? (result.data.monitors as CaptureMonitor[])
            : [];
          if (monitors.length) {
            waitingForWindow = true;
            setPending((previous) =>
              previous.map((candidate) =>
                candidate === item ? { ...item, request, captureChoices: monitors } : candidate,
              ),
            );
            if (!item.voiceRequestId) setAction(null);
            reply = {
              status: 'WAITING_CONFIRMATION',
              message: '캡처할 모니터와 범위를 선택해 주세요.',
              choices: monitors.flatMap((_, i) => [
                { id: 'capture:' + i + ':full', label: '모니터 ' + (i + 1) + ' 전체' },
                { id: 'capture:' + i + ':region', label: '모니터 ' + (i + 1) + ' 영역' },
              ]),
            };
            return;
          }
        }
        if (result.data?.closeRequested && result.data.closed === false)
          setNotice('저장 확인 또는 사용자 조작이 필요합니다');
        if (
          result.success &&
          ['file.search', 'folder.search'].includes(request.commandType) &&
          !item.voiceRequestId
        ) {
          setSearchFolders(request.commandType === 'folder.search');
          setSearchSummary(
            `${result.data?.scope === 'drive_c' ? 'C 드라이브 이름 검색. ' : result.data?.scope === 'home' ? '사용자 폴더에서 검색했습니다. ' : ''}${result.data?.partial ? '시간/개수 제한으로 일부만 검색했습니다. 범위를 좁혀 다시 검색할 수 있습니다. ' : ''}${Number(result.data?.skippedDirectories) > 0 ? '접근할 수 없는 폴더는 건너뛰었습니다.' : ''}`,
          );
          setFileSearchResults(
            Array.isArray(result.data?.results)
              ? (
                  result.data.results as {
                    resourceId: string;
                    displayName: string;
                    relativePath?: string;
                  }[]
                ).filter(
                  (hit) =>
                    typeof hit.resourceId === 'string' && typeof hit.displayName === 'string',
                )
              : [],
          );
        }
        if (result.success && result.data?.captured && typeof result.data.resourceId === 'string')
          setLastCapture({
            resourceId: result.data.resourceId,
            displayName: String(result.data.displayName),
          });
        if (
          result.success &&
          request.commandType === 'clipboard.read' &&
          !item.call &&
          typeof result.data?.text === 'string'
        )
          setClipboardPreview(result.data.text);
        if (
          result.errorCode === 'WINDOW_SELECTION_REQUIRED' ||
          result.errorCode === 'WINDOW_TARGET_STALE'
        ) {
          const values = Array.isArray(result.data?.choices)
            ? (result.data.choices as WindowChoice[])
            : [];
          const next = { ...item, request, windowChoices: values };
          waitingForWindow = true;
          if (mounted.current) {
            setPending((previous) =>
              previous.map((candidate) => (candidate === item ? next : candidate)),
            );
            if (!item.voiceRequestId) setAction(null);
          }
          const message =
            result.errorCode === 'WINDOW_TARGET_STALE'
              ? '선택한 창이 변경되었습니다. 다시 조회해 주세요.'
              : '작업할 창을 선택해 주세요.';
          reply = {
            status: 'WAITING_CONFIRMATION',
            message,
            choices: values.length
              ? values.map((value, index) => ({
                  id: String(index + 1),
                  label: Array.from(value.appName + ' · ' + value.title)
                    .slice(0, 155)
                    .join(''),
                }))
              : [{ id: 'retry', label: '다시 조회' }],
          };
          return;
        }
        if (
          item.voiceRequestId &&
          result.success &&
          ['file.search', 'folder.search'].includes(request.commandType)
        ) {
          const values = (result.data?.results ?? []) as {
            resourceId?: string;
            displayName?: string;
          }[];
          const choices = values
            .filter(
              (value) =>
                typeof value.resourceId === 'string' && typeof value.displayName === 'string',
            )
            .slice(0, 3)
            .map((value, index) => ({
              id: String(index + 1),
              label: `${Array.from(value.displayName!).slice(0, 120).join('')} 열기`,
              request: {
                commandType: request.commandType === 'file.search' ? 'file.open' : 'folder.open',
                arguments: { resourceId: value.resourceId },
                riskLevel: 'SAFE' as const,
                label: `${value.displayName} 열기`,
              },
            }));
          if (choices.length) {
            voiceSearchChoices.current.set(item.voiceRequestId, choices);
            reply = {
              status: 'WAITING_CONFIRMATION',
              message: '열 대상을 번호로 말하거나 눌러 주세요. 아니오라고 하면 취소해요.',
              choices,
            };
          } else reply = { status: 'ERROR', message: '일치하는 대상을 찾지 못했어요.' };
        } else {
          reply = {
            status: (approved || approvalFailure) && !result.success ? 'ERROR' : 'SUCCESS',
            message: approvalFailure
              ? approvalFailure.message
              : deniedByPolicy
                ? '권한 설정에서 차단되었습니다.'
                : !approved
                  ? '작업을 취소했어요.'
                  : result.success &&
                      request.commandType === 'system.volume.mute' &&
                      typeof result.data?.mute === 'boolean'
                    ? result.data.mute
                      ? '음소거했습니다.'
                      : '음소거를 해제했습니다.'
                    : result.message ||
                      (result.success
                        ? '작업을 완료했어요.'
                        : '작업을 완료하지 못했어요. 다시 말씀해 주세요.'),
          };
        }
        const wasCancelled =
          (!approved && !deniedByPolicy && !approvalFailure) ||
          ['CANCELLED', 'CAPTURE_CANCELLED'].includes(result.errorCode ?? '');
        const duration = Math.round(performance.now() - start);
        await agentApi.executionEvent(
          request,
          deniedByPolicy
            ? 'DENIED'
            : wasCancelled
              ? 'CANCELLED'
              : result.success
                ? 'SUCCEEDED'
                : 'FAILED',
        );
        if (item.voice)
          void logVoice(
            request,
            wasCancelled ? 'CANCELLED' : result.success ? 'SUCCESS' : 'FAILED',
            duration,
            result.errorCode,
          );
        if (item.call) {
          const status =
            wasCancelled || request.riskLevel === 'BLOCKED'
              ? 'DENIED'
              : result.success
                ? 'SUCCEEDED'
                : 'FAILED';
          const turn = await agentApi.local(
            item.call,
            status,
            approved,
            duration,
            {
              ...(approvalFailure ? { failureReason: approvalFailure.errorCode } : {}),
              ...(result.success &&
              request.commandType === 'clipboard.read' &&
              typeof result.data?.text === 'string'
                ? { text: result.data.text }
                : {}),
              ...(result.success &&
              ['file.search', 'folder.search'].includes(request.commandType) &&
              Array.isArray(result.data?.results)
                ? {
                    search: {
                      count: result.data.results.length,
                      partial: result.data.partial === true,
                      displayedLocally: true as const,
                    },
                  }
                : {}),
              ...(result.success &&
              request.commandType === 'system.volume.mute' &&
              typeof result.data?.mute === 'boolean'
                ? { mute: result.data.mute }
                : {}),
              summary: wasCancelled
                ? '사용자가 작업을 취소함'
                : result.data?.closeRequested
                  ? result.data.closed
                    ? '선택한 창이 종료된 것을 확인함'
                    : '종료 요청을 전송했으며 저장 확인 또는 사용자 조작이 필요함'
                  : result.success
                    ? 'Local command completed'
                    : 'Local command did not complete',
            },
            request.attemptId,
          );
          animateReply(turn.message);
          if (turn.message)
            void speakAssistant(turn.message.content).catch(() =>
              setNotice('음성 재생에 실패했습니다.'),
            );
          await reconcile(turn.conversationId);
          setPending((previous) => [
            ...previous,
            ...turn.toolCalls.map((call) => ({ call, voice: false })),
          ]);
        }
        if (wasCancelled && !item.voiceRequestId)
          setAction(
            request.commandType === 'screen.capture'
              ? { status: 'error', message: result.message || '화면 캡처를 취소했습니다.' }
              : null,
          );
        else if (!item.voiceRequestId)
          setAction({
            status: result.success ? 'success' : 'error',
            message:
              result.message ||
              (result.success
                ? `${request.label} 완료`
                : `${request.label}: ${result.errorCode || '실행 실패'}`),
          });
      }
    } catch (cause) {
      reply = { status: 'ERROR', message: apiErrorMessage(cause) };
      if (request) await agentApi.executionEvent(request, 'FAILED').catch(() => undefined);
      if (item.voice && request)
        void logVoice(request, 'FAILED', Math.round(performance.now() - start), 'EXECUTION_FAILED');
      if (!item.voiceRequestId) setAction({ status: 'error', message: apiErrorMessage(cause) });
    } finally {
      executionLock.current = false;
      if (showSearchProgress) setSearchingConversation(null);
      if (mounted.current && !waitingForWindow)
        setPending((previous) => previous.filter((candidate) => candidate !== item));
      if (reply)
        await reportVoice(item, reply.status, reply.message, reply.choices).catch((cause) =>
          setNotice(apiErrorMessage(cause)),
        );
    }
  };
  const runVoice = async (text: string) => {
    if (voiceLock.current || executionLock.current) return;
    voiceLock.current = true;
    try {
      const request = classifyVoiceCommand(text);
      if (request.riskLevel === 'BLOCKED') {
        setAction({ status: 'error', message: '지원하지 않거나 차단된 명령입니다. (BLOCKED)' });
        await logVoice(request, 'FAILED', 0, 'BLOCKED');
      } else {
        const preferences = await settingsApi.permissions();
        const permission = preferences.find((value) => value.toolName === request.commandType);
        setPending((previous) => [...previous, { request, voice: true, permission }]);
      }
    } catch (cause) {
      setAction({ status: 'error', message: apiErrorMessage(cause) });
    } finally {
      voiceLock.current = false;
    }
  };
  const current = pending[0];
  const risk =
    current?.request?.riskLevel ??
    (current?.call?.executionLocation === 'LOCAL' ? toolRequest(current.call).riskLevel : 'SAFE');
  const pendingArguments = current?.request?.arguments ?? current?.call?.arguments;
  const resolvingFileOpen =
    (current?.request?.commandType ?? current?.call?.tool) === 'file.open' &&
    !!pendingArguments?.query &&
    !pendingArguments.resourceId &&
    !pendingArguments.path;
  const needsConfirmation =
    !resolvingFileOpen &&
    !current?.call?.denied &&
    !current?.permission?.denied &&
    risk !== 'BLOCKED' &&
    (!!current?.nativeApproval ||
      (risk !== 'CONFIRM' &&
        (current?.call?.requiresConfirmation || current?.permission?.requiresConfirmation)));
  const approvePrepared = async (item: PendingAction, approved: boolean) => {
    if (approvalLock.current || executionLock.current) return;
    if (!approved) {
      void execute(item, false);
      return;
    }
    if (!item.nativeApproval || !item.request || executionLock.current) return;
    approvalLock.current = true;
    try {
      item.request.approvalId = await invoke<string>('authorize_native_tool', {
        pendingId: item.nativeApproval.pendingId,
      });
      await agentApi.executionEvent(item.request, 'APPROVED');
      item.nativeApproval = undefined;
      void execute(item, true);
    } catch (cause) {
      const expired = String(cause).includes('NATIVE_APPROVAL_EXPIRED');
      const message = expired
        ? '승인 시간이 만료되어 실행하지 않았습니다.'
        : '승인을 확인할 수 없어 실행하지 않았습니다. 대상 상태를 확인하고 다시 요청해 주세요.';
      setNotice(message);
      void execute(item, false, {
        success: false,
        errorCode: expired ? 'NATIVE_APPROVAL_EXPIRED' : 'NATIVE_APPROVAL_STALE',
        message,
      });
    } finally {
      approvalLock.current = false;
    }
  };
  const queueLocalRequest = async (request: SystemActionRequest) => {
    if (executionLock.current || pending.length) return;
    setAction(null);
    if (request.commandType === 'clipboard.read') setClipboardPreview(null);
    try {
      const preferences = await settingsApi.permissions();
      if (!mounted.current) return;
      const permission = preferences.find((value) => value.toolName === request.commandType);
      if (!permission || permission.denied) {
        if (import.meta.env.DEV)
          console.debug('[native availability]', {
            tool: request.commandType,
            code: permission ? 'TOOL_PERMISSION_DENIED' : 'UNKNOWN_TOOL',
          });
        setPermissionTarget(permission ? request.commandType : undefined);
        setNotice(permission ? '권한 설정에서 차단되었습니다.' : '현재 사용할 수 없는 기능입니다.');
        return;
      }
      setPending((previous) => [...previous, { request, voice: false, permission }]);
    } catch (cause) {
      setNotice(apiErrorMessage(cause));
    }
  };
  const chooseFile = async (
    item: PendingAction,
    choice: 'cancel' | 'overwrite' | 'rename' | 'retry',
    name?: string,
  ) => {
    if (executionLock.current || fileChoiceLock.current || !item.fileConflict) return;
    const base = item.request || (item.call ? toolRequest(item.call) : null);
    if (!base) return;
    fileChoiceLock.current = true;
    if (!item.voiceRequestId)
      setAction({ status: 'pending', message: '파일 상태를 확인하고 있어요.' });
    try {
      if (choice !== 'overwrite')
        await invoke('discard_file_approval', { approvalId: item.fileConflict.approvalId }).catch(
          () => undefined,
        );
      if (!mounted.current) return;
      if (choice === 'cancel') {
        await execute(item, false);
        return;
      }
      const args = { ...base.arguments };
      delete args.overwriteToken;
      if (choice === 'overwrite')
        args.overwriteToken = await invoke<string>('authorize_file_overwrite', {
          requestId: item.fileConflict.approvalId,
        });
      if (choice === 'rename' && name) {
        if (base.commandType === 'file.rename') args.newName = name;
        else
          args.destinationPath = String(args.destinationPath)
            .split(/[\\/]/)
            .slice(0, -1)
            .concat(name)
            .join('/');
      }
      if (!mounted.current) {
        if (typeof args.overwriteToken === 'string')
          await invoke('discard_file_approval', { approvalId: args.overwriteToken }).catch(
            () => undefined,
          );
        return;
      }
      item.request = {
        ...base,
        arguments: args,
        approvalId: undefined,
        previousAttemptId: base.attemptId,
        attemptId: undefined,
      };
      await execute(item, true);
    } catch {
      if (mounted.current) {
        setPending((previous) =>
          previous.map((candidate) =>
            candidate === item
              ? {
                  ...item,
                  fileProblem: '파일 또는 승인 정보가 변경되었습니다. 다시 확인해 주세요.',
                }
              : candidate,
          ),
        );
        setAction(null);
      }
    } finally {
      fileChoiceLock.current = false;
    }
  };
  const chooseCapture = (item: PendingAction, monitor: CaptureMonitor, mode: 'full' | 'region') => {
    if (executionLock.current) return;
    const base = item.request || (item.call ? toolRequest(item.call) : null);
    if (!base) return;
    item.captureChoices = undefined;
    item.request = {
      ...base,
      arguments: { monitorId: monitor.monitorId, mode },
      previousAttemptId: base.attemptId,
      attemptId: undefined,
      approvalId: undefined,
    };
    void execute(item, true);
  };
  const chooseWindow = (item: PendingAction, choice: string) => {
    if (executionLock.current) return;
    if (choice === 'cancel') {
      void execute(item, false);
      return;
    }
    const base = item.request || (item.call ? toolRequest(item.call) : null);
    if (!base) return;
    const args = { ...base.arguments };
    delete args.targetWindowId;
    if (choice !== 'retry') {
      const selected = item.windowChoices?.[Number(choice) - 1];
      if (!selected) return;
      args.targetWindowId = selected.targetId;
    }
    const next = {
      ...item,
      request: {
        ...base,
        arguments: args,
        previousAttemptId: base.attemptId,
        attemptId: undefined,
        approvalId: undefined,
      },
      windowChoices: undefined,
    };
    // Execute the same pending item so its ticket is submitted only after a real result.
    item.request = next.request;
    void execute(item, true);
  };
  voiceChoiceHandler.current = (id, choice) => {
    if (cancelledVoice.current.has(id) || executionLock.current) return;
    if (current?.voiceRequestId === id && current.windowChoices !== undefined) {
      chooseWindow(current, choice);
      return;
    }
    if (current?.voiceRequestId === id && current.fileConflict) {
      if (choice === 'overwrite' || choice === 'cancel') void chooseFile(current, choice);
      return;
    }
    if (current?.voiceRequestId === id && current.captureChoices) {
      if (choice === 'cancel') {
        void execute(current, false);
        return;
      }
      const match = /^capture:(\d+):(full|region)$/.exec(choice);
      if (match) {
        const monitor = current.captureChoices[Number(match[1])];
        if (monitor) chooseCapture(current, monitor, match[2] as 'full' | 'region');
      }
      return;
    }
    const search = voiceSearchChoices.current.get(id);
    if (search) {
      if (choice === 'cancel') {
        voiceSearchChoices.current.delete(id);
        void reportVoice({ voice: true, voiceRequestId: id }, 'SUCCESS', '선택을 취소했어요.');
        return;
      }
      const selected = search.find((value) => value.id === choice);
      if (!selected) return;
      voiceSearchChoices.current.delete(id);
      void settingsApi
        .permissions()
        .then((preferences) => {
          if (cancelledVoice.current.has(id) || !mounted.current) return;
          const permission = preferences.find(
            (value) => value.toolName === selected.request.commandType,
          );
          setPending((previous) => [
            ...previous,
            { voice: true, voiceRequestId: id, request: selected.request, permission },
          ]);
        })
        .catch(
          () =>
            void reportVoice(
              { voice: true, voiceRequestId: id },
              'ERROR',
              '권한 설정을 확인하지 못했습니다. 다시 시도해 주세요.',
            ),
        );
    } else if (
      current?.voiceRequestId === id &&
      needsConfirmation &&
      ['yes', 'no'].includes(choice)
    ) {
      if (current.nativeApproval) void approvePrepared(current, choice === 'yes');
      else void execute(current, choice === 'yes');
    }
  };
  useEffect(() => {
    if (
      !current ||
      current.fileIntent ||
      current.fileSearchScope ||
      current.windowChoices !== undefined ||
      current.fileConflict ||
      current.captureChoices ||
      current.nativeApproval
    )
      return;
    if (current.call?.denied || current.permission?.denied) {
      void execute(current, false);
      return;
    }
    if (!needsConfirmation) void execute(current, risk !== 'BLOCKED');
    else if (current.voice && isTauri()) {
      if (!current.voiceRequestId) {
        setPending((previous) =>
          previous.map((item) =>
            item === current ? { ...item, voiceRequestId: crypto.randomUUID() } : item,
          ),
        );
        return;
      }
      void invoke('request_tool_confirmation', {
        requestId: current.voiceRequestId,
        message: `${toolLabel(current.request?.commandType || current.call?.tool || '')}\n${confirmationPrompt(current.request || toolRequest(current.call!))}`,
        choices: [
          { id: 'yes', label: '네' },
          { id: 'no', label: '아니오' },
        ],
      }).catch((cause) => {
        setNotice(apiErrorMessage(cause));
        // An unavailable confirmation surface must never implicitly approve an action.
        void execute(current, false);
      });
    }
  }, [current, needsConfirmation, risk]);
  const openVoice = async () => {
    if (isTauri()) {
      try {
        await invoke('activate_voice_orb');
      } catch (cause) {
        setNotice(apiErrorMessage(cause));
      }
    } else setNotice('음성 Orb는 ACE 데스크톱 앱에서 사용할 수 있습니다.');
  };
  const changeWake = async (enabled: boolean) => {
    if (!isTauri()) {
      setWake(enabled);
      return;
    }
    try {
      await invoke('set_wake_word_enabled', { enabled });
      setWake(enabled);
      if (!enabled) setWakeState('disabled');
    } catch (cause) {
      setWakeState('error');
      setNotice(apiErrorMessage(cause));
    }
  };
  const busy =
    mutating || sending || !!current || action?.status === 'pending' || typingReplyIds.size > 0;
  const changeWakeModel = async (source: 'default' | 'personal') => {
    try {
      const settings = await invoke<WakeSetupStatus['settings']>('set_wake_word_model', { source });
      setWake(settings.enabled);
      setWakeReferenceExists(settings.referenceExists);
      setWakeSetupCompleted(settings.setupCompleted);
      setWakeDefaultAvailable(settings.defaultAvailable);
      setWakeModelSource(settings.modelSource);
    } catch (cause) {
      setNotice(apiErrorMessage(cause));
    }
  };
  return (
    <div className="app">
      <CustomTitleBar onNotice={setNotice} />
      <div className="main">
        <div
          ref={sidebarPanel}
          className={`sidebar-panel${collapsed ? ' is-collapsed' : ''}`}
          inert={collapsed}
          aria-hidden={collapsed}
        >
          <Sidebar
            query={conversations}
            profile={user.displayName || user.email}
            active={active}
            onSelect={(id) => {
              if (
                !sending &&
                !current &&
                action?.status !== 'pending' &&
                typingReplyIds.size === 0
              ) {
                deletedConversations.current.delete(id);
                activeRef.current = id;
                setActive(id);
              }
            }}
            onNew={startNewChat}
            onSettings={() => setModal('settings')}
            onCollapse={() => setCollapsed(true)}
            onRename={(conversation) => {
              setRename(conversation);
              setName(conversation.title);
            }}
            onUpdate={(conversation, type) => void update(conversation, type)}
            busy={mutating}
          />
        </div>
        <main className="chat-layout">
          <header className="chat-header">
            {collapsed && (
              <>
                <button
                  className="icon-button"
                  aria-label="사이드바 펼치기"
                  ref={expandSidebar}
                  onClick={() => setCollapsed(false)}
                >
                  <PanelLeftOpen size={19} />
                </button>
              </>
            )}
            <span>
              ACE <span className="header-subtitle">Personal assistant</span>
            </span>
          </header>
          {active || optimistic ? (
            <MessageList
              key={active || 'pending'}
              query={messages}
              sending={sending}
              progressLabel={searchingConversation === active && active ? '찾는 중…' : undefined}
              optimistic={optimistic?.conversationId === active || !active ? optimistic : null}
              animateIds={animateIds}
              onReplyComplete={finishReply}
            />
          ) : (
            <div className="welcome">
              <div className="welcome-inner">
                <div className="welcome-mark">
                  <img src="/ace-logo.png" alt="ACE" />
                </div>
                <p className="eyebrow">A LITTLE HELP. A LOT OF POSSIBILITY.</p>
                <h1>무엇을 도와드릴까요?</h1>
                <p className="welcome-description">
                  궁금한 순간부터, 일상의 작은 작업까지.
                  <br />
                  당신 곁의 AI 어시스턴트, ACE와 함께하세요.
                </p>
                <div className="suggestions">
                  {[
                    {
                      icon: SlidersHorizontal,
                      label: '환경 설정하기',
                      onClick: () => setModal('settings'),
                      sub: 'ACE를 나에게 맞게 설정',
                    },
                    {
                      icon: AudioLines,
                      label: '목소리로 말하기',
                      onClick: () => void openVoice(),
                      sub: '음성 명령 입력',
                    },
                  ].map((item) => (
                    <button key={item.label} disabled={busy} onClick={item.onClick}>
                      <item.icon size={20} />
                      <strong>{item.label}</strong>
                      <span>{item.sub}</span>
                      <ArrowUpRight size={15} className="suggestion-arrow" />
                    </button>
                  ))}
                </div>
              </div>
            </div>
          )}
          {action?.status === 'error' && (
            <div className={`system-action ${action.status}`} role="status">
              <CircleAlert size={17} />
              <span>{action.message}</span>
              {
                <button
                  className="icon-button"
                  aria-label="실행 상태 닫기"
                  onClick={() => setAction(null)}
                >
                  <X size={15} />
                </button>
              }
            </div>
          )}
          <ChatComposer
            hasOlderHistory={messages.hasMore}
            historyLoading={messages.isLoadingMore}
            onLoadOlderHistory={messages.loadMore}
            history={messages.items
              .filter((message) => message.role === 'USER')
              .map((message) => message.content)}
            key={`composer-${active || `draft-${draftVersion}`}`}
            onSend={send}
            sending={sending}
            onStop={stopSend}
            onVoice={() => void openVoice()}
            busy={busy || (!!active && messages.isLoading)}
          />
        </main>
      </div>
      {modal === 'trash' && (
        <TrashModal
          onClose={() => setModal(null)}
          onRestored={(conversation) => {
            deletedConversations.current.delete(conversation.id);
            conversations.upsert(conversation);
          }}
        />
      )}
      {modal === 'settings' && (
        <SettingsModal
          permissionTarget={permissionTarget}
          theme={theme}
          wake={wake}
          wakeState={wakeState}
          onTheme={setTheme}
          onWake={(enabled) => void changeWake(enabled)}
          wakeSetupCompleted={wakeSetupCompleted}
          wakeReferenceExists={wakeReferenceExists}
          wakeDefaultAvailable={wakeDefaultAvailable}
          wakeModelSource={wakeModelSource}
          onWakeModel={(source) => changeWakeModel(source)}
          onWakeSetup={() => setWakeSetupOpen(true)}
          onTrash={() => setModal('trash')}
          onClose={() => setModal(null)}
          onLogout={() => void logout()}
        />
      )}
      {wakeSetupOpen && (
        <WakeWordSetupModal
          onboarding={false}
          onClose={() => {
            setWakeSetupOpen(false);
          }}
          onCompleted={() => {
            localStorage.setItem('ace-onboarding-completed', 'true');
            setWake(true);
            setWakeState('listening');
            setWakeSetupCompleted(true);
            setWakeReferenceExists(true);
            setWakeModelSource('personal');
          }}
        />
      )}
      {rename && (
        <Modal title="대화 이름 변경" onClose={() => setRename(null)}>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              if (name.trim()) void update(rename, 'rename', name.trim());
            }}
          >
            <label className="field-label" htmlFor="conversation-name">
              대화 이름
            </label>
            <input
              id="conversation-name"
              className="text-field"
              autoFocus
              maxLength={200}
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
            <div className="modal-actions">
              <button type="button" onClick={() => setRename(null)}>
                취소
              </button>
              <button className="primary" disabled={!name.trim() || mutating}>
                저장
              </button>
            </div>
          </form>
        </Modal>
      )}
      {current &&
        current.windowChoices === undefined &&
        !current.fileIntent &&
        !current.fileSearchScope &&
        (!current.fileConflict || !!current.nativeApproval) &&
        !current.captureChoices &&
        action?.status !== 'pending' &&
        needsConfirmation &&
        (!current.voice || !isTauri()) && (
          <ToolConfirmation
            request={current.request || (current.call ? toolRequest(current.call) : undefined)}
            view={
              current.nativeApproval?.view ||
              confirmationView(current.request || toolRequest(current.call!))
            }
            key={current.call?.id || current.voiceRequestId || 'main'}
            prompt={
              current.nativeApproval?.description ||
              confirmationPrompt(current.request || toolRequest(current.call!))
            }
            busy={false}
            onChoose={(approved) =>
              current.nativeApproval
                ? void approvePrepared(current, approved)
                : void execute(current, approved)
            }
          />
        )}
      {current?.windowChoices !== undefined && (!current.voice || !isTauri()) && (
        <section className="main-tool-confirmation" role="dialog" aria-label="작업할 창 선택">
          <strong>작업할 창을 선택해 주세요.</strong>
          <div
            style={{ display: 'flex', flexDirection: 'column', maxHeight: 320, overflowY: 'auto' }}
          >
            {current.windowChoices.map((value, index) => (
              <button
                key={value.targetId}
                disabled={action?.status === 'pending'}
                onClick={() => chooseWindow(current, String(index + 1))}
              >
                {value.appName} · {value.title}
              </button>
            ))}
            {!current.windowChoices.length && (
              <>
                <p>선택한 창이 닫혔거나 대상이 변경되었습니다.</p>
                <button onClick={() => chooseWindow(current, 'retry')}>다시 조회</button>
              </>
            )}
            <button
              disabled={action?.status === 'pending'}
              onClick={() => chooseWindow(current, 'cancel')}
            >
              취소
            </button>
          </div>
        </section>
      )}
      {fileSearchResults && (
        <section className="main-tool-confirmation" role="dialog" aria-label="파일 검색 결과">
          <strong>{searchFolders ? '폴더' : '파일'} 검색 결과</strong>
          <p>{searchSummary}</p>
          {!fileSearchResults.length && <p>검색한 범위에서 일치하는 이름을 찾지 못했습니다.</p>}
          <div style={{ maxHeight: '40vh', overflowY: 'auto' }}>
            {fileSearchResults.map((hit) => (
              <div key={hit.resourceId}>
                <p style={{ overflowWrap: 'anywhere' }}>
                  {hit.directory === 'drive_c' ? 'C:\\' : ''}
                  {hit.relativePath || hit.displayName}
                </p>
                {hit.executable === false && (
                  <p>검색 결과만 표시합니다. 이 경로의 실행·변경 권한은 부여되지 않습니다.</p>
                )}
                <button
                  disabled={pending.length > 0 || hit.executable === false}
                  onClick={() => {
                    setFileSearchResults(null);
                    void queueLocalRequest({
                      commandType: searchFolders ? 'folder.open' : 'file.open',
                      arguments: { resourceId: hit.resourceId },
                      label: '파일 열기',
                      riskLevel: 'SAFE',
                      contractVersion: '1.1',
                    });
                  }}
                >
                  {searchFolders ? '폴더 열기' : '파일 열기'}
                </button>
              </div>
            ))}
          </div>
          <button onClick={() => setFileSearchResults(null)}>닫기</button>
        </section>
      )}
      {current?.fileSearchScope && action?.status !== 'pending' && (
        <section className="main-tool-confirmation" role="dialog" aria-label="파일 검색 위치 선택">
          <strong>어디에서 파일을 찾을까요?</strong>
          <div className="approval-actions" style={{ flexWrap: 'wrap' }}>
            {(
              [
                ['desktop', '바탕화면'],
                ['downloads', '다운로드'],
                ['documents', '문서'],
                ['pictures', '사진'],
                ['music', '음악'],
                ['videos', '동영상'],
              ] as const
            ).map(([directory, label]) => (
              <button
                key={directory}
                onClick={() => {
                  if (executionLock.current) return;
                  const base = current.request || (current.call ? toolRequest(current.call) : null);
                  if (!base) return;
                  setPending((previous) =>
                    previous.map((candidate) =>
                      candidate === current
                        ? {
                            ...current,
                            fileSearchScope: undefined,
                            request: {
                              ...base,
                              arguments: { ...base.arguments, directory },
                              previousAttemptId: base.attemptId,
                              attemptId: undefined,
                              approvalId: undefined,
                            },
                          }
                        : candidate,
                    ),
                  );
                }}
              >
                {label}
              </button>
            ))}
            <button onClick={() => void execute(current, false)}>취소</button>
          </div>
        </section>
      )}
      {current?.fileIntent && action?.status !== 'pending' && (
        <FileTargetResolver
          tool={current.request?.commandType || current.call!.tool}
          intent={current.fileIntent}
          onCancel={() => void execute(current, false)}
          onResolve={(args) => {
            if (executionLock.current) return;
            const base = current.request || (current.call ? toolRequest(current.call) : null);
            if (!base) return;
            current.fileIntent = undefined;
            current.request = {
              ...base,
              arguments: args,
              previousAttemptId: base.attemptId,
              attemptId: undefined,
              approvalId: undefined,
            };
            void execute(current, true);
          }}
        />
      )}
      {current?.fileConflict && !current.nativeApproval && action?.status !== 'pending' && (
        <FileConflict
          key={current.fileConflict.approvalId}
          conflict={current.fileConflict}
          operation={current.request?.commandType || current.call?.tool}
          problem={current.fileProblem}
          onChoose={(choice, name) => void chooseFile(current, choice, name)}
        />
      )}
      {current?.captureChoices && action?.status !== 'pending' && (
        <section
          className="main-tool-confirmation capture-monitor-picker"
          role="dialog"
          aria-label="캡처 모니터 선택"
          onKeyDown={(event) => {
            if (event.key === 'Escape') void execute(current, false);
          }}
        >
          <strong>어느 화면을 캡처할까요?</strong>
          {current.captureChoices.map((monitor, index) => (
            <div key={monitor.monitorId} className="capture-monitor-option">
              <span>
                모니터 {index + 1} · {monitor.width}×{monitor.height}
              </span>
              <div className="approval-actions">
                <button
                  aria-label={`모니터 ${index + 1} 전체 캡처`}
                  onClick={() => chooseCapture(current, monitor, 'full')}
                >
                  전체 화면
                </button>
                <button
                  aria-label={`모니터 ${index + 1} 영역 선택`}
                  onClick={() => chooseCapture(current, monitor, 'region')}
                >
                  영역 선택
                </button>
              </div>
            </div>
          ))}
          <button onClick={() => void execute(current, false)}>취소</button>
        </section>
      )}
      {lastCapture && (
        <div className="capture-result" role="status">
          <span>화면 캡처 완료</span>
          <label>
            저장 파일 이름{' '}
            <input
              aria-label="캡처 저장 파일 이름"
              value={captureSaveName}
              onChange={(e) => setCaptureSaveName(e.target.value)}
            />
          </label>
          <button
            disabled={captureSaving}
            onClick={async () => {
              if (captureSaveLock.current) return;
              captureSaveLock.current = true;
              setCaptureSaving(true);
              try {
                const saved = await invoke<string>('save_capture_file', {
                  resourceId: lastCapture.resourceId,
                  name: captureSaveName.trim(),
                });
                setNotice(`이미지 저장 완료: ${saved}`);
              } catch (error) {
                setNotice(String(error));
              } finally {
                captureSaveLock.current = false;
                setCaptureSaving(false);
              }
            }}
          >
            다른 이름으로 저장
          </button>
          <span>사진 폴더의 ACE Captures에 저장됩니다. 닫으면 임시 이미지를 정리합니다.</span>
          <button
            disabled={pending.length > 0 || executionLock.current}
            onClick={() =>
              void queueLocalRequest({
                commandType: 'file.open',
                arguments: { resourceId: lastCapture.resourceId },
                label: '캡처 이미지 열기',
                riskLevel: 'SAFE',
              })
            }
          >
            이미지 열기
          </button>
          <button
            disabled={captureSaving || pending.length > 0}
            aria-label="캡처 결과 닫기"
            onClick={() => setLastCapture(null)}
          >
            닫기
          </button>
        </div>
      )}
      {clipboardPreview !== null && (
        <section className="main-tool-confirmation" role="dialog" aria-label="클립보드 내용">
          <strong>클립보드 내용</strong>
          <p>
            채팅 요청으로 읽은 내용은 AI 답변을 위해 전달됩니다. 직접 테스트한 내용은 이 창에서만
            표시합니다.
          </p>
          <textarea readOnly aria-label="읽은 클립보드 내용" value={clipboardPreview} />
          <button onClick={() => setClipboardPreview(null)}>닫기</button>
        </section>
      )}
      {notice && (
        <div className="toast" role="status">
          <span>{notice}</span>
          {notice === '권한 설정에서 차단되었습니다.' && (
            <button
              onClick={() => {
                setModal('settings');
                setNotice('');
              }}
            >
              권한 설정으로 이동
            </button>
          )}
          <button className="icon-button" aria-label="알림 닫기" onClick={() => setNotice('')}>
            <X size={15} />
          </button>
        </div>
      )}
    </div>
  );
}
export default function App() {
  return (
    <AuthGate>{(user, logout) => <AceApp key={user.id} user={user} logout={logout} />}</AuthGate>
  );
}
