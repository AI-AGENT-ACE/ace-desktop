import { useId, useState } from 'react';
import { ChevronDown } from 'lucide-react';
import type { Permission, PermissionPolicy } from '../../types';

const names: Record<string, string> = {
  'weather.current': '날씨 조회',
  'app.open': '앱 실행',
  'app.close': '앱 종료',
  'file.open': '파일 열기',
  'file.rename': '파일 이름 변경',
  'file.delete': '파일 삭제',
};
const policies: { value: PermissionPolicy; label: string }[] = [
  { value: 'ALWAYS_ALLOW', label: '항상 허용' },
  { value: 'ASK', label: '확인' },
  { value: 'ALWAYS_ASK', label: '항상 확인' },
];
export function PermissionSettings({
  permissions,
  busy,
  onChange,
}: {
  permissions: Permission[];
  busy: boolean;
  onChange: (permission: Permission, policy: PermissionPolicy) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const id = useId();
  return (
    <section className="permission-settings">
      <button
        type="button"
        className="permission-heading"
        aria-expanded={expanded}
        aria-controls={id}
        onClick={() => setExpanded(!expanded)}
      >
        <div>
          <strong>권한 설정하기</strong>
          <span>도구별 실행 방식을 선택하세요.</span>
        </div>
        <ChevronDown size={18} className={expanded ? 'expanded' : ''} aria-hidden="true" />
      </button>
      <div id={id} hidden={!expanded} className="permission-list">
        {permissions.map((permission) => (
          <fieldset key={permission.toolName} className="permission-item" disabled={busy}>
            <legend>{names[permission.toolName] || permission.toolName}</legend>
            <div className="permission-choices">
              {policies.map((policy) => (
                <label key={policy.value} className="permission-choice">
                  <input
                    type="radio"
                    name={`${id}-${permission.toolName}`}
                    value={policy.value}
                    checked={permission.policy === policy.value}
                    onChange={() => onChange(permission, policy.value)}
                  />
                  <span>{policy.label}</span>
                </label>
              ))}
            </div>
            {permission.systemConfirmation && (
              <p className="permission-note">이 항목은 항상 실행 전 승인이 필요합니다.</p>
            )}
          </fieldset>
        ))}
      </div>
    </section>
  );
}
