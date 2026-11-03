import { useState } from 'react';
export type ApprovalTarget = { label: string; name: string; path?: string; detail?: string };
export type ApprovalView = {
  question: string;
  action: string;
  targets: ApprovalTarget[];
  warning?: string;
};
function displayPath(path: string) {
  return path.replace(/^\\\\\?\\UNC\\/, '\\\\').replace(/^\\\\\?\\/, '');
}
function compactPath(path: string) {
  const shown = displayPath(path);
  const parts = shown.split(/[\\/]/);
  if (parts.length < 5) return shown;
  const separator = shown.includes('\\') ? '\\' : '/';
  return [...parts.slice(0, shown.startsWith('\\\\') ? 4 : 2), '…', ...parts.slice(-2)].join(
    separator,
  );
}
export function TargetDetails({ target }: { target: ApprovalTarget }) {
  const [copied, setCopied] = useState('');
  return (
    <div className="approval-target">
      <span className="approval-target-label">{target.label}</span>
      <p className="approval-target-name">{target.name}</p>
      {target.detail && <p className="approval-target-detail">{target.detail}</p>}
      {target.path && (
        <>
          <p className="approval-path">{compactPath(target.path)}</p>
          <div className="approval-path-actions">
            <details>
              <summary>전체 경로 보기</summary>
              <div className="approval-full-path" tabIndex={0}>
                {target.path}
              </div>
            </details>
            <button
              type="button"
              onClick={async () => {
                try {
                  await navigator.clipboard.writeText(target.path!);
                  setCopied('경로를 복사했습니다.');
                } catch {
                  setCopied('복사하지 못했습니다. 전체 경로를 펼쳐 직접 복사해 주세요.');
                }
              }}
            >
              경로 복사
            </button>
          </div>
          {copied && (
            <p className="permission-note" role="status">
              {copied}
            </p>
          )}
        </>
      )}
    </div>
  );
}
