import { useEffect, useRef } from 'react';

export function ToolConfirmation({
  label,
  busy,
  onChoose,
}: {
  label: string;
  busy: boolean;
  onChoose: (approved: boolean) => void;
}) {
  const cancel = useRef<HTMLButtonElement>(null);
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
      <strong id="tool-confirmation-title">실행을 허용할까요?</strong>
      <p>{label}</p>
      <div>
        <button disabled={busy} onClick={() => onChoose(true)}>
          허용
        </button>
        <button ref={cancel} disabled={busy} onClick={() => onChoose(false)}>
          취소
        </button>
      </div>
    </section>
  );
}
