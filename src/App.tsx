import { useEffect, useRef, useState } from 'react';
import {
  ArrowUpRight,
  AudioLines,
  Check,
  CloudSun,
  CircleAlert,
  LoaderCircle,
  PanelLeftOpen,
  SlidersHorizontal,
  X,
} from 'lucide-react';
import { isTauri, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { CustomTitleBar } from './layouts/CustomTitleBar';
import { Sidebar } from './layouts/Sidebar';
import { Modal } from './components/Modal';
import { ChatComposer } from './features/chat/ChatComposer';
import { MessageList } from './features/chat/MessageList';
import { TrashModal } from './features/trash/TrashModal';
import { VoiceOverlay } from './features/voice/VoiceOverlay';
import { AuthGate } from './features/auth/AuthGate';
import { SettingsModal } from './features/settings/SettingsModal';
import {
  classifyVoiceCommand,
  systemAdapter,
  toolRequest,
} from './features/system-actions/adapters/systemAdapter';
import { conversationApi } from './api/conversations.api';
import { messagesApi } from './api/messages.api';
import { agentApi } from './api/agent.api';
import { recordVoiceLog } from './api/voice.api';
import { apiErrorMessage, isCancelled } from './api/client';
import { useConversations } from './hooks/useConversations';
import { useMessages } from './hooks/useMessages';
import type {
  Conversation,
  SystemActionRequest,
  SystemActionStatus,
  ToolCall,
  User,
  VoiceState,
} from './types';
import './styles/globals.css';
import './styles/motion.css';

type Action = { status: SystemActionStatus; message: string };
type PendingAction = { request?: SystemActionRequest; call?: ToolCall; voice: boolean };
function AceApp({ user, logout }: { user: User; logout: () => Promise<void> }) {
  const conversations = useConversations();
  const [active, setActive] = useState<string | null>(null);
  const [draftVersion, setDraftVersion] = useState(0);
  const messages = useMessages(active);
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
  const [wake, setWake] = useState(() => localStorage.getItem('ace-wake') !== 'off');
  const [voice, setVoice] = useState<VoiceState>('idle');
  const [transcript, setTranscript] = useState('');
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
  useEffect(() => {
    mounted.current = true;
    const controller = new AbortController();
    agentApi
      .health(controller.signal)
      .then((data) => setAiReady(data.ai === 'configured'))
      .catch((cause) => {
        if (!isCancelled(cause)) setNotice(apiErrorMessage(cause));
      });
    return () => {
      mounted.current = false;
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
    localStorage.setItem('ace-wake', wake ? 'on' : 'off');
  }, [wake]);
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
    void register<boolean>('ace-wake-word-changed', (enabled) => {
      if (typeof enabled === 'boolean') setWake(enabled);
    });
    void register('ace-open-settings', () => {
      setModal('settings');
      void invoke('take_pending_settings_request');
    });
    void invoke('set_wake_word_enabled', { enabled: wake }).catch((cause) =>
      setNotice(apiErrorMessage(cause)),
    );
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
    setDraftVersion((version) => version + 1);
    setNotice('');
  };
  const reconcile = async (id: string) => {
    const page = await messagesApi.list(id);
    if (mounted.current && id === active) page.items.slice().reverse().forEach(messages.append);
    return page.items;
  };
  const send = async (text: string): Promise<boolean> => {
    if (lock.current) return false;
    lock.current = true;
    setSending(true);
    let id = active;
    const known = new Set(messages.items.map((message) => message.id));
    try {
      if (!id) {
        const conversation = await conversationApi.create();
        id = conversation.id;
        conversations.upsert(conversation);
      }
      const configured = aiReady ?? (await agentApi.health()).ai === 'configured';
      setAiReady(configured);
      if (configured) {
        const turn = await agentApi.turn(id, text);
        await reconcile(id);
        if (mounted.current)
          setPending((previous) => [
            ...previous,
            ...turn.toolCalls.map((call) => ({ call, voice: false })),
          ]);
      } else {
        const saved = await messagesApi.create(id, text);
        if (id === active) messages.append(saved);
        setNotice('메시지를 저장했습니다. AI 응답은 AI 서버 연결 후 사용할 수 있습니다.');
      }
      if (mounted.current) {
        setActive(id);
        conversations.upsert(await conversationApi.get(id));
      }
      return true;
    } catch (cause) {
      if (mounted.current) setNotice(apiErrorMessage(cause));
      // An AI failure can happen after the USER message was committed. Never retry the POST blindly.
      if (id) {
        try {
          const saved = await reconcile(id);
          if (
            saved.some(
              (message) =>
                message.role === 'USER' && message.content === text && !known.has(message.id),
            )
          ) {
            if (mounted.current) {
              setActive(id);
              conversations.upsert(await conversationApi.get(id));
            }
            return true;
          }
        } catch {
          /* Keep the original send error and the draft. */
        }
      }
      return false;
    } finally {
      lock.current = false;
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
    executionLock.current = true;
    const start = performance.now();
    const request = item.request || (item.call ? toolRequest(item.call) : null);
    setAction({ status: 'pending', message: '명령 처리 중…' });
    try {
      if (item.call?.executionLocation === 'CLOUD') {
        if (!approved) {
          setAction({ status: 'error', message: '도구 실행을 취소했습니다.' });
          return;
        }
        const turn = await agentApi.cloud(item.call, true);
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
        const duration = Math.round(performance.now() - start);
        if (item.voice)
          await logVoice(
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
      if (item.voice && request)
        await logVoice(
          request,
          'FAILED',
          Math.round(performance.now() - start),
          'EXECUTION_FAILED',
        );
      setAction({ status: 'error', message: apiErrorMessage(cause) });
    } finally {
      executionLock.current = false;
      if (mounted.current)
        setPending((previous) => previous.filter((candidate) => candidate !== item));
    }
  };
  const runVoice = async (text: string) => {
    if (voiceLock.current || executionLock.current) return;
    voiceLock.current = true;
    setTranscript('');
    setVoice('idle');
    try {
      const request = classifyVoiceCommand(text);
      if (request.riskLevel === 'BLOCKED') {
        setAction({ status: 'error', message: '지원하지 않거나 차단된 명령입니다. (BLOCKED)' });
        await logVoice(request, 'FAILED', 0, 'BLOCKED');
      } else if (request.riskLevel === 'CONFIRM')
        setPending((previous) => [...previous, { request, voice: true }]);
      else await execute({ request, voice: true }, true);
    } finally {
      voiceLock.current = false;
    }
  };
  const current = pending[0];
  const risk =
    current?.request?.riskLevel ??
    (current?.call?.executionLocation === 'LOCAL' ? toolRequest(current.call).riskLevel : 'SAFE');
  const needsConfirmation =
    risk !== 'BLOCKED' && (risk === 'CONFIRM' || current?.call?.requiresConfirmation);
  useEffect(() => {
    if (current && !needsConfirmation) void execute(current, risk !== 'BLOCKED');
  }, [current, needsConfirmation, risk]);
  const openVoice = async () => {
    if (isTauri()) {
      try {
        await invoke('activate_voice_orb');
      } catch (cause) {
        setNotice(apiErrorMessage(cause));
      }
    } else setVoice('listening');
  };
  const changeWake = async (enabled: boolean) => {
    if (!isTauri()) {
      setWake(enabled);
      return;
    }
    try {
      await invoke('set_wake_word_enabled', { enabled });
      setWake(enabled);
    } catch (cause) {
      setNotice(apiErrorMessage(cause));
    }
  };
  const busy = mutating || sending || !!current || action?.status === 'pending';
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
            onTrash={() => setModal('trash')}
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
          {active ? (
            <MessageList key={`messages-${active}`} query={messages} sending={sending} />
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
                      icon: CloudSun,
                      label: '오늘 날씨 알아보기',
                      onClick: () => void send('오늘 날씨 알려줘'),
                      sub: '오늘의 날씨를 물어보세요',
                    },
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
          {action && (
            <div className={`system-action ${action.status}`} role="status">
              {action.status === 'pending' ? (
                <LoaderCircle size={17} className="spin" />
              ) : action.status === 'success' ? (
                <Check size={17} />
              ) : (
                <CircleAlert size={17} />
              )}
              <span>{action.message}</span>
              {action.status !== 'pending' && (
                <button
                  className="icon-button"
                  aria-label="실행 상태 닫기"
                  onClick={() => setAction(null)}
                >
                  <X size={15} />
                </button>
              )}
            </div>
          )}
          <ChatComposer
            key={`composer-${active || `draft-${draftVersion}`}`}
            onSend={send}
            onVoice={() => void openVoice()}
            busy={busy || (!!active && messages.isLoading)}
            wake={wake}
          />
        </main>
      </div>
      {voice !== 'idle' && (
        <VoiceOverlay
          state={voice}
          transcript={transcript}
          onTranscript={setTranscript}
          onFinish={() => void runVoice(transcript.trim())}
          onClose={() => {
            setVoice('idle');
            setTranscript('');
          }}
        />
      )}
      {modal === 'trash' && (
        <TrashModal onClose={() => setModal(null)} onChanged={() => void conversations.refresh()} />
      )}
      {modal === 'settings' && (
        <SettingsModal
          theme={theme}
          wake={wake}
          onTheme={setTheme}
          onWake={(enabled) => void changeWake(enabled)}
          onTrash={() => setModal('trash')}
          onClose={() => setModal(null)}
          onLogout={() => void logout()}
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
      {current && needsConfirmation && (
        <Modal title="시스템 명령 실행 확인" onClose={() => void execute(current, false)}>
          <p className="modal-description">ACE가 다음 작업을 실행하려고 합니다.</p>
          <div className="command-preview">{current.request?.label || current.call?.tool}</div>
          <p className="modal-description">승인하면 실제 작업이 실행됩니다.</p>
          <div className="modal-actions">
            <button
              disabled={action?.status === 'pending'}
              onClick={() => void execute(current, false)}
            >
              취소
            </button>
            <button
              className="primary"
              disabled={action?.status === 'pending'}
              onClick={() => void execute(current, true)}
            >
              실행
            </button>
          </div>
        </Modal>
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
