import { useState } from 'react';
import { TargetDetails } from './TargetDetails';
function extension(path: string) {
  const leaf = path.split(/[\\/]/).pop() || '';
  const dot = leaf.lastIndexOf('.');
  return dot > 0 && dot < leaf.length - 1 ? leaf.slice(dot) : '';
}
export type ConflictInfo = {
  approvalId: string;
  source: string;
  target: string;
  sourceSize: number;
  targetSize: number;
  expiresInSeconds: number;
};
export function FileConflict({
  conflict,
  operation,
  problem,
  onChoose,
}: {
  conflict: ConflictInfo;
  operation?: string;
  problem?: string;
  onChoose: (choice: 'cancel' | 'overwrite' | 'rename' | 'retry', name?: string) => void;
}) {
  const [name, setName] = useState('');
  const [confirmedName, setConfirmedName] = useState('');
  const trimmed = name.trim();
  const sourceExtension = extension(conflict.source);
  const enteredExtension = extension(trimmed);
  const finalName = trimmed ? trimmed + (enteredExtension ? '' : sourceExtension) : '';
  const extensionChanged =
    !!enteredExtension && enteredExtension.toLowerCase() !== sourceExtension.toLowerCase();
  const invalid =
    !trimmed ||
    /[\\/:*?"<>|\x00-\x1f]/.test(trimmed) ||
    trimmed.endsWith('.') ||
    /^(CON|PRN|AUX|NUL|COM[1-9¹²³]|LPT[1-9¹²³])(?:\.|$)/i.test(finalName) ||
    finalName.length > 255;
  return (
    <section
      className="main-tool-confirmation file-conflict"
      role="dialog"
      aria-label="파일 덮어쓰기 확인"
      onKeyDown={(e) => {
        if (e.key === 'Escape') {
          e.stopPropagation();
          onChoose('cancel');
        }
      }}
    >
      <strong>이 파일을 덮어쓸까요?</strong>
      <TargetDetails
        target={{
          label: '원본',
          name: conflict.source.split(/[\\/]/).pop() || conflict.source,
          path: conflict.source,
          detail: `${conflict.sourceSize.toLocaleString()}바이트`,
        }}
      />
      <TargetDetails
        target={{
          label: '대상',
          name: conflict.target.split(/[\\/]/).pop() || conflict.target,
          path: conflict.target,
          detail: `${conflict.targetSize.toLocaleString()}바이트`,
        }}
      />
      <p className="approval-warning">대상 파일의 기존 내용이 바뀝니다.</p>
      {problem && <p role="alert">{problem}</p>}
      <div className="approval-actions">
        <button onClick={() => onChoose('cancel')}>취소</button>
        <button disabled={!!problem} onClick={() => onChoose('overwrite')}>
          덮어쓰기
        </button>
        {problem && <button onClick={() => onChoose('retry')}>다시 확인</button>}
      </div>
      <label>
        다른 이름 지정
        <input
          aria-label="다른 파일 이름"
          value={name}
          onChange={(e) => {
            setName(e.target.value);
            setConfirmedName('');
          }}
          aria-describedby="conflict-final-name"
          placeholder="새 파일 이름"
          maxLength={200}
        />
      </label>
      <p id="conflict-final-name" aria-live="polite">
        실제 저장될 파일명: <strong>{finalName || '이름을 입력해 주세요.'}</strong>
      </p>
      {extensionChanged && !invalid && (
        <div>
          <p>
            확장자를 {sourceExtension || '(없음)'}에서 {enteredExtension}로 변경합니다. 확장자를
            바꿔도 파일 내용이나 형식이 변환되지는 않습니다.
          </p>
          <label>
            <input
              type="checkbox"
              checked={confirmedName === finalName}
              onChange={(e) => setConfirmedName(e.target.checked ? finalName : '')}
            />
            확장자 변경 확인
          </label>
        </div>
      )}
      <button
        disabled={invalid || (extensionChanged && confirmedName !== finalName)}
        onClick={() => onChoose('rename', finalName)}
      >
        {operation === 'file.move' ? '다른 이름으로 이동' : operation === 'file.rename' ? '이름 바꾸기' : '다른 이름으로 복사'}
      </button>
    </section>
  );
}
