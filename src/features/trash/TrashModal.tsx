import { useEffect, useRef, useState } from 'react';
import { Trash2, RotateCcw } from 'lucide-react';
import { Modal } from '../../components/Modal';
import { conversationApi } from '../../api/conversations.api';
import { apiErrorMessage } from '../../api/client';
import { useTrashConversations } from '../../hooks/useConversations';
import { useInfiniteScroll } from '../../hooks/useInfiniteScroll';
import type { TrashConversation } from '../../types';
export function TrashModal({ onClose, onChanged }: { onClose: () => void; onChanged: () => void }) {
  const query = useTrashConversations();
  const [confirm, setConfirm] = useState<TrashConversation | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const lock = useRef(false);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const sentinel = useInfiniteScroll(
    query.loadMore,
    query.hasMore && !query.isLoading && !query.isLoadingMore && !query.error,
  );
  const mutate = async (id: string, permanent: boolean) => {
    if (lock.current) return;
    lock.current = true;
    setBusy(true);
    setError('');
    try {
      if (permanent) await conversationApi.permanentlyDelete(id);
      else await conversationApi.restore(id);
      if (mounted.current) {
        query.remove(id);
        setConfirm(null);
        onChanged();
      }
    } catch (cause) {
      if (mounted.current) setError(apiErrorMessage(cause));
    } finally {
      lock.current = false;
      if (mounted.current) setBusy(false);
    }
  };
  return (
    <Modal title="휴지통" onClose={onClose}>
      <p className="modal-description">삭제한 대화는 30일 동안 복구할 수 있습니다.</p>
      {error && (
        <p className="danger-text" role="alert">
          {error}
        </p>
      )}
      {confirm ? (
        <div className="confirm-body">
          <Trash2 size={28} />
          <h3>대화를 영구 삭제할까요?</h3>
          <p>‘{confirm.title}’ 대화와 메시지가 삭제됩니다. 이 작업은 되돌릴 수 없습니다.</p>
          <div className="modal-actions">
            <button disabled={busy} onClick={() => setConfirm(null)}>
              취소
            </button>
            <button
              className="danger-button"
              disabled={busy}
              onClick={() => void mutate(confirm.id, true)}
            >
              {busy ? '삭제 중…' : '영구 삭제'}
            </button>
          </div>
        </div>
      ) : (
        <div className="trash-list">
          {query.isLoading ? (
            <p className="state-text">삭제된 대화를 불러오는 중…</p>
          ) : (
            <>
              {query.error && (
                <p role="alert">
                  {query.error}
                  <button onClick={() => void query.refresh()}>다시 시도</button>
                </p>
              )}
              {!query.items.length && !query.error && (
                <div className="trash-empty">
                  <Trash2 size={32} />
                  <h3>휴지통이 비어 있습니다</h3>
                  <p>삭제한 대화가 이곳에 표시됩니다.</p>
                </div>
              )}
              {query.items.map((conversation) => (
                <div key={conversation.id} className="trash-item">
                  <div>
                    <strong>{conversation.title}</strong>
                    <p>
                      삭제: {new Date(conversation.deletedAt!).toLocaleDateString('ko-KR')} ·{' '}
                      {conversation.isRestorable
                        ? `${conversation.remainingDays}일 후 삭제 대상`
                        : '복구 기간 만료'}
                    </p>
                  </div>
                  <button
                    aria-label={`${conversation.title} 복구`}
                    disabled={busy || !conversation.isRestorable}
                    onClick={() => void mutate(conversation.id, false)}
                  >
                    <RotateCcw size={15} />
                    복구
                  </button>
                  <button
                    className="icon-button danger-text"
                    aria-label={`${conversation.title} 영구 삭제`}
                    disabled={busy}
                    onClick={() => setConfirm(conversation)}
                  >
                    <Trash2 size={17} />
                  </button>
                </div>
              ))}
              {query.hasMore && (
                <button
                  className="load-more"
                  disabled={query.isLoadingMore}
                  onClick={() => void query.loadMore()}
                >
                  {query.isLoadingMore ? '불러오는 중…' : '휴지통 더 보기'}
                </button>
              )}
              <div ref={sentinel} />
            </>
          )}
        </div>
      )}
    </Modal>
  );
}
