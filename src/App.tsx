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
import { ToolConfirmation } from './features/system-actions/ToolConfirmation';

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
  localPolicy,
  systemAdapter,
  toolRequest,
  toolLabel,
} from './features/system-actions/adapters/systemAdapter';
import { conversationApi } from './api/conversations.api';
import { messagesApi } from './api/messages.api';
import { attachmentsApi } from './api/attachments.api';
import { settingsApi } from './api/settings.api';
import { agentApi } from './api/agent.api';
import { speakAssistant, stopSpeech } from './api/speech.api';
import { recordVoiceLog } from './api/voice.api';
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
type PendingAction = {
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
  const sendAbort = useRef<AbortController | null>(null);
  const animateReply = (message: Message | null) => {
    if (message) setAnimateIds((previous) => new Set([...previous, message.id]));
  };
  const stopSend = () => {
    sendAbort.current?.abort();
    stopSpeech();
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
  const [pending, setPending] = useState<PendingAction[]>([]);
  const [sending, setSending] = useState(false);
  const [mutating, setMutating] = useState(false);
  const [notice, setNotice] = useState('');
  const [aiReady, setAiReady] = useState<boolean | null>(null);
  const lock = useRef(false);
  const mutationLock = useRef(false);
  const executionLock = useRef(false);
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
        conversations.remove(conversation.id);
        if (active === conversation.id) setActive(null);
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
    const page = await messagesApi.list(id);
    if (mounted.current && id === activeRef.current)
      page.items.slice().reverse().forEach(appendRef.current);
    return page.items;
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
      saved = await messagesApi.create(id, text, uploaded);
      setOptimistic(saved);
      appendRef.current(saved);
      conversations.upsert(await conversationApi.get(id));
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
        conversations.upsert(await conversationApi.get(id));
      } else if (!configured)
        setNotice('메시지를 저장했습니다. AI 서버 연결 후 답변을 받을 수 있습니다.');
      return true;
    } catch (cause) {
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
      await recordVoiceLog({
        commandType: request.commandType,
        status,
        duration,
        ...(errorCode ? { errorCode } : {}),
      });
    } catch (cause) {
      if (mounted.current) setNotice(`음성 실행 로그 저장 실패: ${apiErrorMessage(cause)}`);
    }
  };
  const execute = async (item: PendingAction, approved: boolean) => {
    if (executionLock.current) return;
    if (item.voiceRequestId && cancelledVoice.current.has(item.voiceRequestId)) return;
    executionLock.current = true;
    const start = performance.now();
    const request = item.request || (item.call ? toolRequest(item.call) : null);
    let reply: { status: string; message: string; choices?: VoiceChoice[] } | null = null;
    setAction({ status: 'pending', message: '명령 처리 중…' });
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
        const result = approved
          ? await systemAdapter.execute(request, true)
          : { success: false, errorCode: 'CANCELLED' };
        if (item.voiceRequestId && cancelledVoice.current.has(item.voiceRequestId)) return;
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
            status: approved && !result.success ? 'ERROR' : 'SUCCESS',
            message: !approved
              ? '작업을 취소했어요.'
              : result.message ||
                (result.success
                  ? '작업을 완료했어요.'
                  : '작업을 완료하지 못했어요. 다시 말씀해 주세요.'),
          };
        }
        const duration = Math.round(performance.now() - start);
        if (item.voice)
          void logVoice(
            request,
            !approved ? 'CANCELLED' : result.success ? 'SUCCESS' : 'FAILED',
            duration,
            result.errorCode,
          );
        if (item.call) {
          const status =
            !approved || request.riskLevel === 'BLOCKED'
              ? 'DENIED'
              : result.success
                ? 'SUCCEEDED'
                : 'FAILED';
          const turn = await agentApi.local(item.call, status, approved, duration, {
            summary: result.success ? 'Local command completed' : 'Local command did not complete',
          });
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
      if (item.voice && request)
        void logVoice(request, 'FAILED', Math.round(performance.now() - start), 'EXECUTION_FAILED');
      setAction({ status: 'error', message: apiErrorMessage(cause) });
    } finally {
      executionLock.current = false;
      if (mounted.current)
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
  const needsConfirmation =
    !current?.call?.denied &&
    !current?.permission?.denied &&
    risk !== 'BLOCKED' &&
    (risk === 'CONFIRM' ||
      current?.call?.requiresConfirmation ||
      current?.permission?.requiresConfirmation);
  voiceChoiceHandler.current = (id, choice) => {
    if (cancelledVoice.current.has(id) || executionLock.current) return;
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
      void execute(current, choice === 'yes');
    }
  };
  useEffect(() => {
    if (!current) return;
    if (current.call?.denied || current.permission?.denied) {
      void reportVoice(current, 'ERROR', '설정에서 허용하지 않은 기능입니다.');
      setAction({ status: 'error', message: '설정에서 허용하지 않은 기능입니다.' });
      setPending((previous) => previous.filter((candidate) => candidate !== current));
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
        message:
          (current.request?.label || toolLabel(current.call!.tool, current.call!.arguments)) +
          ' 작업을 실행할까요?',
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
  const busy = mutating || sending || !!current || action?.status === 'pending';
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
              if (!busy) setActive(id);
            }}
            onNew={startNewChat}
            onSettings={() => setModal('settings')}
            onCollapse={() => setCollapsed(true)}
            onRename={(conversation) => {
              setRename(conversation);
              setName(conversation.title);
            }}
            onUpdate={(conversation, type) => void update(conversation, type)}
            busy={busy}
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
              optimistic={optimistic?.conversationId === active || !active ? optimistic : null}
              animateIds={animateIds}
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
        <TrashModal onClose={() => setModal(null)} onChanged={() => void conversations.refresh()} />
      )}
      {modal === 'settings' && (
        <SettingsModal
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
      {current && needsConfirmation && (!current.voice || !isTauri()) && (
        <ToolConfirmation
          key={current.call?.id || current.voiceRequestId || 'main'}
          label={current.request?.label || toolLabel(current.call!.tool, current.call!.arguments)}
          busy={action?.status === 'pending'}
          onChoose={(approved) => void execute(current, approved)}
        />
      )}
      {notice && (
        <div className="toast" role="status">
          {notice}
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
