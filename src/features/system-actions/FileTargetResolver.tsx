import { useEffect, useRef, useState } from 'react';
import { systemAdapter } from './adapters/systemAdapter';
import { settingsApi } from '../../api/settings.api';
const roots = [
  ['desktop', '바탕화면'],
  ['downloads', '다운로드'],
  ['documents', '문서'],
  ['pictures', '사진'],
  ['music', '음악'],
  ['videos', '동영상'],
];
const text = (value: unknown) =>
  typeof value === 'string'
    ? value
    : typeof (value as { original?: unknown })?.original === 'string'
      ? String((value as { original: string }).original)
      : '';
type Hit = { resourceId: string; displayName: string; relativePath?: string };
// Interpret only known roots and relative paths; Rust remains the path authority.
function location(value: string) {
  let path = value.trim().replace(/\\/g, '/');
  let directory = '';
  for (const [id, label] of roots) {
    const match = path.match(new RegExp(`^(?:${label}|${id})(?:\\s+|/|의\\s*)`, 'i'));
    if (match) {
      directory = id;
      path = path.slice(match[0].length);
      break;
    }
  }
  return { directory, path: path.replace(/\s*폴더(?:의|\s+안의)\s*/g, '/').trim() };
}
export function FileTargetResolver({
  tool,
  intent,
  onResolve,
  onCancel,
}: {
  tool: string;
  intent: Record<string, unknown>;
  onResolve: (args: Record<string, unknown>) => void;
  onCancel: () => void;
}) {
  const original = text(intent.query);
  const source = location(original);
  const target = location(text(intent.destination) || text(intent.newName));
  const destinationEdited = useRef(false);
  const explicitDirectory =
    typeof intent.directory === 'string'
      ? intent.directory
      : (intent.directory as { canonicalId?: string } | undefined)?.canonicalId ||
        roots.find(([id, label]) => [id, label].includes(text(intent.directory)))?.[0];
  const [directory, setDirectory] = useState(
    roots.some(([id]) => id === explicitDirectory)
      ? explicitDirectory!
      : source.directory || 'desktop',
  );
  // This is only an editable search hint, never an automatically executed path.
  const [query, setQuery] = useState(source.path);
  const [destinationDirectory, setDestinationDirectory] = useState(
    target.directory || source.directory || 'desktop',
  );
  const [destinationPath, setDestinationPath] = useState(target.path);
  const [hits, setHits] = useState<Hit[]>([]);
  const [selected, setSelected] = useState<Hit | null>(null);
  const [problem, setProblem] = useState('');
  const [busy, setBusy] = useState(false);
  const [submitted, setSubmitted] = useState(false);
  const generation = useRef(0);
  useEffect(
    () => () => {
      generation.current++;
    },
    [],
  );
  const clear = () => {
    generation.current++;
    setBusy(false);
    setSelected(null);
    setHits([]);
  };
  function choose(hit: Hit) {
    setSelected(hit);
    if (
      target.path &&
      !destinationEdited.current &&
      !target.directory &&
      !/[\\/]/.test(target.path)
    ) {
      const parent = (hit.relativePath || '').replace(/\\/g, '/').split('/').slice(0, -1).join('/');
      setDestinationDirectory(directory);
      setDestinationPath(
        tool === 'file.rename' ? target.path : [parent, target.path].filter(Boolean).join('/'),
      );
    }
  }
  async function search(automatic = false) {
    const id = ++generation.current;
    setBusy(true);
    setProblem('');
    setHits([]);
    setSelected(null);
    try {
      const permissions = await settingsApi.permissions();
      if (id !== generation.current) return;
      const permission = permissions.find((value) => value.toolName === 'file.search');
      if (!permission || permission.denied) {
        setProblem(
          permission
            ? '파일 검색이 권한 설정에서 차단되어 있습니다.'
            : '연결한 백엔드에 파일 검색 도구가 등록되지 않았습니다. 백엔드 업데이트가 필요합니다.',
        );
        return;
      }
      if (automatic && permission.requiresConfirmation) {
        setProblem('파일 검색 권한 확인이 필요합니다. 파일 찾기를 눌러 주세요.');
        return;
      }
      const result = await systemAdapter.execute({
        commandType: 'file.search',
        arguments: {
          directory,
          query: query.trim(),
          extensions: intent.extensions || [],
          limit: 50,
        },
        label: '파일 검색',
        riskLevel: 'SAFE',
      });
      if (id !== generation.current) return;
      if (!result.success) {
        setProblem(result.message || '파일 검색에 실패했습니다.');
        return;
      }
      const values = Array.isArray(result.data?.results) ? (result.data.results as Hit[]) : [];
      setHits(values);
      if (values.length === 1) choose(values[0]);
      if (!values.length) setProblem('파일을 찾지 못했습니다. 위치와 검색어를 확인해 주세요.');
    } catch {
      if (id === generation.current) setProblem('파일 검색에 실패했습니다. 다시 시도해 주세요.');
    } finally {
      if (id === generation.current) setBusy(false);
    }
  }
  useEffect(() => {
    if (query.trim()) void search(true);
    // The dialog mounts for one intent. Edits use the explicit search button.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const rename = tool === 'file.rename';
  const transfer = tool === 'file.copy' || tool === 'file.move';
  const invalid =
    (!destinationPath.trim() && (rename || tool === 'file.move')) ||
    (rename && /[\\/:*?"<>|]/.test(destinationPath));
  return (
    <section
      className="main-tool-confirmation file-target-resolver"
      role="dialog"
      aria-label="파일 대상 확인"
      onKeyDown={(e) => {
        if (e.key === 'Escape') onCancel();
      }}
    >
      <strong>실행할 파일과 위치를 확인해 주세요</strong>
      <p>AI 요청: {original}</p>
      <label>
        찾을 위치
        <select
          aria-label="찾을 위치"
          value={directory}
          onChange={(e) => {
            clear();
            setDirectory(e.target.value);
          }}
        >
          {roots.map(([value, label]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
      </label>
      <label>
        파일 검색어
        <input
          aria-label="파일 검색어"
          value={query}
          onChange={(e) => {
            clear();
            setQuery(e.target.value);
          }}
        />
      </label>
      <button disabled={busy || !query.trim() || submitted} onClick={() => void search()}>
        파일 찾기
      </button>
      <div className="file-target-results">
        {hits.map((hit) => (
          <button
            key={hit.resourceId}
            aria-pressed={selected?.resourceId === hit.resourceId}
            onClick={() => choose(hit)}
          >
            {hit.relativePath || hit.displayName}
          </button>
        ))}
      </div>
      {selected && <p>선택한 파일: {selected.relativePath || selected.displayName}</p>}
      {tool === 'file.copy' && !destinationPath.trim() && (
        <p>같은 폴더에 원본 이름의 복사본을 만듭니다. 대상 경로는 비워 두어도 됩니다.</p>
      )}
      {transfer && (
        <label>
          대상 위치
          <select
            aria-label="대상 위치"
            value={destinationDirectory}
            onChange={(e) => {
              destinationEdited.current = true;
              setDestinationDirectory(e.target.value);
            }}
          >
            {roots.map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
        </label>
      )}
      {(transfer || rename) && (
        <label>
          {rename ? '새 파일 이름' : '대상 위치 아래 경로와 파일 이름'}
          <input
            aria-label="대상 파일 경로"
            placeholder={rename ? '대상.txt' : 'ACE테스트/대상.txt'}
            value={destinationPath}
            onChange={(e) => {
              destinationEdited.current = true;
              setDestinationPath(e.target.value);
            }}
          />
        </label>
      )}
      {busy && <p role="status">요청한 위치에서 파일을 찾고 있어요.</p>}
      {problem && <p role="alert">{problem}</p>}
      <button
        disabled={!selected || invalid || submitted}
        onClick={() => {
          if (!selected || submitted) return;
          setSubmitted(true);
          onResolve({
            resourceId: selected.resourceId,
            ...(rename
              ? { newName: destinationPath.trim() }
              : transfer && destinationPath.trim()
                ? { destinationDirectory, destinationPath: destinationPath.trim() }
                : {}),
          });
        }}
      >
        선택한 파일로 진행
      </button>
      <button onClick={onCancel}>취소</button>
    </section>
  );
}
