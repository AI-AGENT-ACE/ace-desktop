import { useEffect, useRef, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import type { SystemActionRequest } from '../../types';
import { normalizeAppArguments } from './adapters/systemAdapter';
import { TargetDetails, type ApprovalView } from './TargetDetails';

export function ToolConfirmation({
  prompt,
  view,
  request,
  busy,
  onChoose,
}: {
  prompt: string;
  view?: ApprovalView;
  request?: SystemActionRequest;
  busy: boolean;
  onChoose: (approved: boolean) => void;
}) {
  const cancel = useRef<HTMLButtonElement>(null);
  const [resolvedState, setResolved] = useState<{ key: string; view: ApprovalView }>();
  const targetKey = JSON.stringify([request?.commandType, request?.arguments]);
  const resolved = resolvedState?.key === targetKey ? resolvedState.view : undefined;
  const [error, setError] = useState('');
  const needsFile =
    !!request && ['file.open', 'app.open'].includes(request.commandType) && isTauri();
  useEffect(() => {
    setResolved(undefined);
    setError('');
    let active = true;
    if (needsFile)
      void invoke<ApprovalView>('describe_native_target', {
        tool: request!.commandType,
        arguments: normalizeAppArguments(request!.commandType, request!.arguments),
      })
        .then((value) => {
          if (active) setResolved({ key: targetKey, view: value });
        })
        .catch(() => {
          if (active) setError('대상 정보를 확인하지 못했습니다. 취소 후 다시 선택해 주세요.');
        });
    return () => {
      active = false;
    };
  }, [needsFile, targetKey]);
  const shown = resolved || view;
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    cancel.current?.focus();
    return () => {
      if (previous?.isConnected) previous.focus();
    };
  }, []);
  return (
    <section
      className="main-tool-confirmation"
      role="dialog"
      aria-modal="false"
      aria-labelledby="tool-confirmation-title"
      onKeyDown={(event) => {
        if (event.key === 'Escape' && !busy) {
          event.stopPropagation();
          onChoose(false);
        }
      }}
    >
      <strong id="tool-confirmation-title">{shown?.question || prompt}</strong>
      {shown?.targets.map((target, index) => (
        <TargetDetails key={index} target={target} />
      ))}
      {shown?.warning && <p className="approval-warning">{shown.warning}</p>}
      {error && <p role="alert">{error}</p>}
      {needsFile && !resolved && !error && <p role="status">대상 확인 중…</p>}
      <div className="approval-actions">
        <button ref={cancel} disabled={busy} onClick={() => onChoose(false)}>
          취소
        </button>
        <button disabled={busy || (needsFile && !resolved)} onClick={() => onChoose(true)}>
          {shown?.action || '실행'}
        </button>
      </div>
    </section>
  );
}
