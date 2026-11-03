import { useEffect, useId, useRef, useState } from 'react';
import { ChevronDown } from 'lucide-react';
import type { Permission, PermissionPolicy } from '../../types';
import {
  toolGroups,
  toolPresentation,
  toolPresentations,
} from '../system-actions/toolPresentation';
const policies: { value: PermissionPolicy; label: string }[] = [
  { value: 'DENY', label: '차단' },
  { value: 'ASK', label: '실행 전 확인' },
  { value: 'ALWAYS_ALLOW', label: '항상 허용' },
];
export function PermissionSettings({
  targetTool,
  permissions,
  busy,
  onChange,
}: {
  permissions: Permission[];
  targetTool?: string;
  busy: boolean;
  onChange: (permission: Permission, policy: PermissionPolicy) => void;
}) {
  const [expanded, setExpanded] = useState(!!targetTool);
  const target = useRef<HTMLFieldSetElement>(null);
  useEffect(() => {
    if (targetTool) {
      setExpanded(true);
      target.current?.scrollIntoView({ block: 'center' });
      target.current?.focus();
    }
  }, [targetTool, permissions]);
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
        <p className="permission-description">
          항상 허용: 일반적인 실행 확인을 생략합니다. 파일 삭제·덮어쓰기 등 별도 확인이 필요한
          작업은 다시 물어볼 수 있습니다.
        </p>
        {toolGroups.map((group) => {
          const items = permissions
            .filter((p) => toolPresentation(p.toolName).group === group)
            .sort(
              (a, b) =>
                Object.keys(toolPresentations).indexOf(a.toolName) -
                Object.keys(toolPresentations).indexOf(b.toolName),
            );
          if (!items.length) return null;
          return (
            <section className="permission-group" key={group} aria-label={group}>
              <h4>{group}</h4>
              {items.map((permission) => (
                <fieldset ref={permission.toolName === targetTool ? target : undefined} tabIndex={-1} key={permission.toolName} className="permission-item" disabled={busy}>
                  <legend>{toolPresentation(permission.toolName).name}</legend>
                  <p className="permission-description">
                    {toolPresentation(permission.toolName).description}
                  </p>
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
            </section>
          );
        })}
      </div>
    </section>
  );
}
