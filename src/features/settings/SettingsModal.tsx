import { useEffect, useRef, useState } from 'react';
import { Trash2 } from 'lucide-react';
import appPackage from '../../../package.json';
import { settingsApi } from '../../api/settings.api';
import { apiErrorMessage, isCancelled } from '../../api/client';
import { Modal } from '../../components/Modal';
import type { AgentSettings, Permission, PermissionPolicy } from '../../types';
import { PermissionSettings } from './PermissionSettings';
import { WakeSensitivitySettings } from '../voice/WakeSensitivitySettings';
import { WakeChimeSettings } from '../voice/WakeChimeSettings';
export function SettingsModal({
  theme,
  wake,
  wakeState,
  onTheme,
  onWake,

  wakeReferenceExists,

  wakeModelSource,

  onWakeSetup,
  onTrash,
  onClose,
  onLogout,
}: {
  theme: string;
  wake: boolean;
  wakeState: 'disabled' | 'starting' | 'listening' | 'triggered' | 'paused' | 'error';
  onTheme: (value: string) => void;
  onWake: (value: boolean) => void;
  wakeSetupCompleted: boolean;
  wakeReferenceExists: boolean;
  wakeDefaultAvailable: boolean;
  wakeModelSource: 'default' | 'personal' | null;
  onWakeModel: (source: 'default' | 'personal') => Promise<void>;
  onWakeSetup: () => void;
  onTrash: () => void;
  onClose: () => void;
  onLogout: () => void;
}) {
  const [settings, setSettings] = useState<AgentSettings | null>(null);
  const [permissions, setPermissions] = useState<Permission[]>([]);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const lock = useRef(false);
  const mounted = useRef(true);
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    mounted.current = true;
    const controller = new AbortController();
    setError('');
    Promise.all([settingsApi.get(controller.signal), settingsApi.permissions(controller.signal)])
      .then(([data, policies]) => {
        if (!controller.signal.aborted) {
          setSettings(data);
          setPermissions(policies);
        }
      })
      .catch((cause) => {
        if (!isCancelled(cause) && !controller.signal.aborted) setError(apiErrorMessage(cause));
      });
    return () => {
      mounted.current = false;
      controller.abort();
    };
  }, [retry]);
  const update = async (operation: () => Promise<void>) => {
    if (lock.current) return;
    lock.current = true;
    setBusy(true);
    setError('');
    try {
      await operation();
    } catch (cause) {
      if (mounted.current) setError(apiErrorMessage(cause));
    } finally {
      lock.current = false;
      if (mounted.current) setBusy(false);
    }
  };
  const save = (next: AgentSettings) =>
    void update(async () => {
      const saved = await settingsApi.update(next);
      if (mounted.current) setSettings(saved);
    });
  const savePermission = (permission: Permission, policy: PermissionPolicy) =>
    void update(async () => {
      setPermissions((previous) =>
        previous.map((item) =>
          item.toolName === permission.toolName
            ? {
                ...item,
                policy,
                denied: policy === 'DENY',
                requiresConfirmation:
                  policy !== 'DENY' && (item.systemConfirmation || policy === 'ASK'),
              }
            : item,
        ),
      );
      try {
        const saved = await settingsApi.permission(permission.toolName, policy);
        if (mounted.current)
          setPermissions((previous) =>
            previous.map((item) => (item.toolName === saved.toolName ? saved : item)),
          );
      } catch (cause) {
        if (mounted.current)
          setPermissions((previous) =>
            previous.map((item) => (item.toolName === permission.toolName ? permission : item)),
          );
        throw cause;
      }
    });
  return (
    <Modal title="설정" onClose={onClose}>
      <p className="modal-description">ACE를 나에게 맞게 설정하세요.</p>
      {error && (
        <p className="danger-text" role="alert">
          {error}
          <button onClick={() => setRetry((value) => value + 1)}>다시 조회</button>
        </p>
      )}
      <div className="setting-row">
        <div>
          <strong>화면 테마</strong>
          <p>이 PC에 저장됩니다.</p>
        </div>
        <select
          aria-label="화면 테마"
          value={theme}
          onChange={(event) => onTheme(event.target.value)}
        >
          <option value="light">라이트</option>
          <option value="dark">다크</option>
        </select>
      </div>
      <div className="setting-row">
        <div>
          <strong>음성 호출 · Wake Word</strong>
          <p>
            {wakeState === 'starting'
              ? '마이크와 음성 감지 엔진을 시작하고 있습니다.'
              : wakeState === 'listening'
                ? '음성 감지 중입니다. “ACE”라고 불러보세요.'
                : wakeState === 'triggered'
                  ? '호출어를 감지했습니다.'
                  : wakeState === 'paused'
                    ? '음성 입력 중에는 호출어 감지를 잠시 멈춥니다.'
                    : wakeState === 'error'
                      ? '음성 감지를 시작하지 못했습니다. Tray에서 다시 시도할 수 있습니다.'
                      : wakeModelSource === 'personal'
                        ? '저장된 개인 보정으로 “ACE” 호출을 감지합니다.'
                        : wakeReferenceExists
                          ? '녹음 없이 켜고 “ACE”라고 불러보세요.'
                          : '기본 호출 모델이 없습니다. 개인 보정을 사용하거나 기본 모델이 포함된 앱을 설치해 주세요.'}
          </p>
        </div>
        <button
          className={`toggle ${wake ? 'on' : ''}`}
          role="switch"
          aria-checked={wake}
          aria-label="Wake Word 활성화"
          disabled={!wakeReferenceExists && !wake}
          onClick={() => onWake(!wake)}
        >
          <span />
        </button>
      </div>
      <div className="setting-row wake-personal-row">
        <div>
          <strong>음성 호출</strong>
          <p>
            녹음 없이 바로 ACE를 불러보세요. 잘 인식되지 않으면 내 목소리로 5번 녹음해 보정할 수
            있습니다. 녹음은 선택 사항입니다.
          </p>
        </div>
        <button className="wake-record-button" onClick={onWakeSetup}>
          내 목소리로 ACE 호출어 녹음
        </button>
      </div>
      <WakeSensitivitySettings />
      <WakeChimeSettings />
      {!settings ? (
        <p className="state-text">계정 설정 불러오는 중…</p>
      ) : (
        <>
          <div className="setting-row">
            <div>
              <strong>응답 언어</strong>
              <p>계정 설정으로 저장됩니다.</p>
            </div>
            <select
              aria-label="응답 언어"
              value={settings.responseLanguage}
              disabled={busy}
              onChange={(event) => save({ ...settings, responseLanguage: event.target.value })}
            >
              <option value="ko">한국어</option>
              <option value="en">English</option>
              {!['ko', 'en'].includes(settings.responseLanguage) && (
                <option value={settings.responseLanguage}>{settings.responseLanguage}</option>
              )}
            </select>
          </div>
          <div className="setting-row">
            <div>
              <strong>음성 응답 선호</strong>
              <p>채팅 답변을 음성으로 읽어줍니다.</p>
            </div>
            <button
              className={`toggle ${settings.ttsEnabled ? 'on' : ''}`}
              role="switch"
              aria-label="음성 응답 선호"
              aria-checked={settings.ttsEnabled}
              disabled={busy}
              onClick={() => save({ ...settings, ttsEnabled: !settings.ttsEnabled })}
            >
              <span />
            </button>
          </div>
        </>
      )}
      <PermissionSettings permissions={permissions} busy={busy} onChange={savePermission} />
      <div className="setting-row">
        <div>
          <strong>삭제된 대화</strong>
          <p>30일 동안 대화를 복구할 수 있어요.</p>
        </div>
        <button className="settings-navigation" onClick={onTrash}>
          <Trash2 size={15} />
          휴지통 보기
        </button>
      </div>
      <div className="modal-actions">
        <button onClick={onLogout}>로그아웃</button>
      </div>
      <p className="settings-footnote">
        ACE {appPackage.version} · © {new Date().getFullYear()} ACE. All rights reserved.
      </p>
    </Modal>
  );
}
