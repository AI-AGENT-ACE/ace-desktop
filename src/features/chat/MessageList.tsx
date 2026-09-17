import { useLayoutEffect, useRef } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { ArrowDown, LoaderCircle } from 'lucide-react';
import type { useMessages } from '../../hooks/useMessages';
export function MessageList({
  query,
  sending,
}: {
  query: ReturnType<typeof useMessages>;
  sending: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const nearBottom = useRef(true);
  const previous = useRef<{ height: number; top: number } | null>(null);
  const initialized = useRef(false);
  const { items: messages } = query;
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
    <div
      className="message-area"
      ref={ref}
      style={{ overflowAnchor: 'none' }}
      onScroll={() => {
        const element = ref.current!;
        nearBottom.current = element.scrollHeight - element.scrollTop - element.clientHeight < 100;
        if (element.scrollTop < 45 && !query.error) older();
      }}
    >
      <div className="chat-content message-content">
        {query.isLoading ? (
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
                    <ReactMarkdown remarkPlugins={[remarkGfm]}>{message.content}</ReactMarkdown>
                  ) : (
                    <p>{message.content}</p>
                  )}
                </div>
              </article>
            ))}
            {sending && (
              <p className="state-text" role="status">
                <LoaderCircle size={16} className="spin" />
                메시지 처리 중…
              </p>
            )}
          </>
        )}
      </div>
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
    </div>
  );
}
