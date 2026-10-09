import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {
  ArrowDown,
  Check,
  Copy,
  Download,
  FileAudio,
  FileText,
  Image,
  LoaderCircle,
} from 'lucide-react';
import type { Message } from '../../types';
import type { useMessages } from '../../hooks/useMessages';
import { attachmentsApi } from '../../api/attachments.api';
// A saved reply is typed only on its first presentation, including when revisiting a room.
const presentedReplies = new Set<string>();
export function MessageList({
  query,
  sending,
  optimistic,
  animateIds,
}: {
  query: ReturnType<typeof useMessages>;
  sending: boolean;
  optimistic?: Message | null;
  animateIds?: Set<string>;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const nearBottom = useRef(true);
  const [showJump, setShowJump] = useState(false);
  const previous = useRef<{ height: number; top: number } | null>(null);
  const initialized = useRef(false);
  const allMessages =
    optimistic && !query.items.some((item) => item.id === optimistic.id)
      ? [...query.items, optimistic]
      : query.items;
  const messages = allMessages.filter((message) => message.role !== 'TOOL');
  const updateJump = () => {
    const element = ref.current;
    if (element) setShowJump(element.scrollHeight - element.scrollTop - element.clientHeight > 100);
  };
  useEffect(() => {
    const element = ref.current;
    const content = element?.firstElementChild;
    if (!element || !content) return;
    const observer = new ResizeObserver(() => {
      if (nearBottom.current && !previous.current) element.scrollTop = element.scrollHeight;
      updateJump();
    });
    observer.observe(content);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    if (previous.current && !query.isLoadingMore) {
      element.scrollTop = previous.current.top + element.scrollHeight - previous.current.height;
      previous.current = null;
    } else if ((!initialized.current || nearBottom.current) && !query.isLoadingMore) {
      element.scrollTop = element.scrollHeight;
      if (!query.isLoading) initialized.current = true;
    }
    updateJump();
  }, [messages, sending, query.isLoading, query.isLoadingMore]);
  const older = () => {
    if (
      query.hasMore &&
      !query.isLoading &&
      !query.isLoadingMore &&
      ref.current &&
      !previous.current
    ) {
      previous.current = { height: ref.current.scrollHeight, top: ref.current.scrollTop };
      void query.loadMore();
    }
  };
  return (
    <div className="message-scroll-shell">
      <div
        className="message-area"
        ref={ref}
        style={{ overflowAnchor: 'none' }}
        onScroll={() => {
          const element = ref.current!;
          nearBottom.current =
            element.scrollHeight - element.scrollTop - element.clientHeight < 100;
          updateJump();
          if (element.scrollTop < 45 && !query.error) older();
        }}
      >
        <div className="chat-content message-content">
          {query.isLoading && !optimistic ? (
            <p className="state-text">
              <LoaderCircle size={17} className="spin" />
              대화를 불러오는 중…
            </p>
          ) : (
            <>
              {query.error && (
                <div className="state-text" role="alert">
                  {query.error}
                  <button onClick={() => void query.refresh()}>다시 시도</button>
                </div>
              )}
              {query.hasMore && (
                <button className="older-button" disabled={query.isLoadingMore} onClick={older}>
                  {query.isLoadingMore ? '이전 메시지 불러오는 중…' : '이전 메시지 보기'}
                </button>
              )}
              {!messages.length && !query.error && (
                <p className="state-text">새로운 대화를 시작해 보세요.</p>
              )}
              {messages.map((message) => (
                <article key={message.id} className={`message ${message.role.toLowerCase()}`}>
                  <div className="message-author">
                    {message.role === 'ASSISTANT' ? (
                      <>
                        <img src="/ace-logo.png" alt="" />
                        ACE
                      </>
                    ) : message.role === 'USER' ? (
                      '나'
                    ) : message.role === 'TOOL' ? (
                      '도구 실행'
                    ) : (
                      '시스템'
                    )}
                  </div>
                  <div className="markdown">
                    {message.role === 'ASSISTANT' ? (
                      <AssistantContent
                        id={message.id}
                        content={message.content}
                        animate={animateIds?.has(message.id) ?? false}
                      />
                    ) : (
                      <p>{message.content}</p>
                    )}
                  </div>
                  {!!message.attachments?.length && (
                    <div className="message-attachments">
                      {message.attachments.map((attachment) => (
                        <button
                          key={attachment.id}
                          onClick={() => void attachmentsApi.download(attachment)}
                          title={`${attachment.originalName} 다운로드`}
                        >
                          {attachment.mimeType.startsWith('image/') ? (
                            <Image size={17} />
                          ) : attachment.mimeType.startsWith('audio/') ? (
                            <FileAudio size={17} />
                          ) : (
                            <FileText size={17} />
                          )}
                          <span>{attachment.originalName}</span>
                          <small>{Math.max(1, Math.ceil(attachment.size / 1024))}KB</small>
                          <Download size={15} />
                        </button>
                      ))}
                    </div>
                  )}
                  {message.role === 'USER' && message.content && (
                    <UserCopy content={message.content} />
                  )}
                </article>
              ))}
              {sending && (
                <article
                  className="message assistant thinking-message"
                  role="status"
                  aria-label="ACE가 답변을 생각하는 중"
                >
                  <div className="message-author">
                    <img src="/ace-logo.png" alt="" />
                    ACE
                  </div>
                  <div className="thinking-bubble">
                    <span className="thinking-dots" aria-hidden="true">
                      <i />
                      <i />
                      <i />
                    </span>
                    생각 중…
                  </div>
                </article>
              )}
            </>
          )}
        </div>
      </div>
      {showJump && (
        <button
          className="jump-bottom icon-button"
          aria-label="최신 메시지로 이동"
          onClick={() => {
            nearBottom.current = true;
            ref.current?.scrollTo({ top: ref.current.scrollHeight, behavior: 'smooth' });
          }}
        >
          <ArrowDown size={18} />
        </button>
      )}
    </div>
  );
}

function AssistantContent({
  id,
  content,
  animate,
}: {
  id: string;
  content: string;
  animate: boolean;
}) {
  const [shouldAnimate] = useState(animate && !presentedReplies.has(id));
  const [length, setLength] = useState(shouldAnimate ? 0 : Array.from(content).length);
  const [copyState, setCopyState] = useState('응답 복사');
  const characters = Array.from(content);
  useEffect(() => {
    presentedReplies.add(id);
    if (!shouldAnimate || window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setLength(Array.from(content).length);
      return;
    }
    const total = Array.from(content).length;
    const step = Math.max(1, Math.ceil(total / 180));
    const timer = setInterval(
      () =>
        setLength((current) => {
          if (current + step >= total) clearInterval(timer);
          return Math.min(total, current + step);
        }),
      20,
    );
    return () => clearInterval(timer);
  }, [content, shouldAnimate, id]);
  const complete = length >= characters.length;
  return (
    <>
      <ReactMarkdown remarkPlugins={[remarkGfm]}>
        {characters.slice(0, length).join('')}
      </ReactMarkdown>
      {!complete && <span className="typing-cursor" aria-hidden="true" />}
      {complete && (
        <div className="response-actions">
          <button
            aria-label="응답 복사"
            onClick={() =>
              void navigator.clipboard.writeText(content).then(
                () => setCopyState('복사됨'),
                () => setCopyState('복사하지 못했습니다'),
              )
            }
          >
            {copyState === '복사됨' ? <Check size={14} /> : <Copy size={14} />}
            {copyState}
          </button>
          <span className="response-divider" aria-hidden="true">
            |
          </span>
          <span>응답 완료</span>
        </div>
      )}
    </>
  );
}

function UserCopy({ content }: { content: string }) {
  const [state, setState] = useState('복사');
  return (
    <div className="user-copy-actions">
      <button
        type="button"
        aria-label="사용자 메시지 복사"
        onClick={() => {
          void navigator.clipboard.writeText(content).then(
            () => setState('복사됨'),
            () => setState('복사 실패'),
          );
        }}
      >
        {state === '복사됨' ? <Check size={14} /> : <Copy size={14} />}
        {state}
      </button>
    </div>
  );
}
