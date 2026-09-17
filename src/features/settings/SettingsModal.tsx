import { useEffect, useRef, useState } from 'react';
import { settingsApi } from '../../api/settings.api';
import { apiErrorMessage, isCancelled } from '../../api/client';
import { Modal } from '../../components/Modal';
import type { AgentSettings, Permission, PermissionPolicy } from '../../types';
export function SettingsModal({
  theme,
  wake,
  onTheme,
  onWake,
  onTrash,
  onClose,
  onLogout,
}: {
  theme: string;
  wake: boolean;
  onTheme: (value: string) => void;
  onWake: (value: boolean) => void;
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
          <strong>Wake Word</strong>
          <p>환경설정만 저장하며 음성 감지는 아직 연결되지 않았습니다.</p>
        </div>
        <button
          className={`toggle ${wake ? 'on' : ''}`}
          role="switch"
          aria-checked={wake}
          aria-label="Wake Word 활성화"
          onClick={() => onWake(!wake)}
        >
          <span />
        </button>
      </div>
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
              <p>TTS 선호도만 저장합니다. 음성 출력은 아직 연결되지 않았습니다.</p>
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
      {permissions.map((permission) => (
        <div className="setting-row" key={permission.toolName}>
          <div>
            <strong>{permission.toolName}</strong>
            <p>
              {permission.systemConfirmation
                ? '시스템 정책에 따라 실행 전 승인이 필요합니다.'
                : '도구 실행 정책'}
            </p>
          </div>
          <select
            aria-label={`${permission.toolName} 권한`}
            value={permission.policy}
            disabled={busy}
            onChange={(event) => {
              const policy = event.target.value as PermissionPolicy;
              void update(async () => {
                const saved = await settingsApi.permission(permission.toolName, policy);
                if (mounted.current)
                  setPermissions((previous) =>
                    previous.map((item) => (item.toolName === saved.toolName ? saved : item)),
                  );
              });
            }}
          >
            <option value="ALWAYS_ALLOW">항상 허용</option>
            <option value="ASK">확인</option>
            <option value="ALWAYS_ASK">항상 확인</option>
          </select>
        </div>
      ))}
      <div className="setting-row">
        <div>
          <strong>삭제된 대화</strong>
          <p>30일 동안 대화를 복구할 수 있어요.</p>
        </div>
        <button onClick={onTrash}>휴지통 열기</button>
      </div>
      <div className="modal-actions">
        <button onClick={onLogout}>로그아웃</button>
      </div>
      <p className="settings-footnote">ACE 0.1.0 · 계정 데이터는 서버에 저장됩니다.</p>
    </Modal>
  );
}
