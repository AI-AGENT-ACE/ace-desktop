import { useEffect, useRef, useState } from 'react';
import {
  Plus,
  Search,
  Pin,
  MoreHorizontal,
  Trash2,
  PanelLeftClose,
  MessageSquare,
  Pencil,
  PinOff,
} from 'lucide-react';
import { useInfiniteScroll } from '../hooks/useInfiniteScroll';
import type { useConversations } from '../hooks/useConversations';
import type { Conversation } from '../types';
import { ProfileMenu } from './ProfileMenu';
export function Sidebar({
  query,
  profile,
  active,
  onSelect,
  onNew,
  onSettings,
  onTrash,
  onCollapse,
  onRename,
  onUpdate,
  busy,
}: {
  query: ReturnType<typeof useConversations>;
  profile: string;
  active: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
  onSettings: () => void;
  onTrash: () => void;
  onCollapse: () => void;
  onRename: (c: Conversation) => void;
  onUpdate: (c: Conversation, action: 'pin' | 'delete') => void;
  busy: boolean;
}) {
  const [search, setSearch] = useState('');
  const [menu, setMenu] = useState<string | null>(null);
  const container = useRef<HTMLElement>(null);
  const items = query.items
    .slice()
    .sort(
      (a, b) =>
        Number(b.isPinned) - Number(a.isPinned) ||
        Date.parse(b.updatedAt) - Date.parse(a.updatedAt),
    )
    .filter((item) => item.title.toLocaleLowerCase().includes(search.toLocaleLowerCase()));
  const sentinel = useInfiniteScroll(
    query.loadMore,
    query.hasMore && !query.isLoading && !query.isLoadingMore && !query.error,
  );
  useEffect(() => {
    if (!menu) return;
    const close = (event: PointerEvent) => {
      if (!(event.target as HTMLElement).closest('.conversation-menu-anchor')) setMenu(null);
    };
    document.addEventListener('pointerdown', close);
    return () => document.removeEventListener('pointerdown', close);
  }, [menu]);
  const loadMore = () => {
    void query.loadMore();
  };
  return (
    <aside className="sidebar" ref={container}>
      <div className="sidebar-top">
        <div className="sidebar-brand">
          <img src="/ace-logo.png" alt="ACE 로고" />
          <strong>ACE</strong>
          <button className="icon-button" aria-label="사이드바 접기" onClick={onCollapse}>
            <PanelLeftClose size={18} />
          </button>
        </div>
        <button className="new-chat" onClick={onNew} disabled={busy}>
          <Plus size={19} />새 채팅
        </button>
        <label className="search">
          <Search size={16} />
          <input
            placeholder="불러온 대화 검색"
            aria-label="대화 검색"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </label>
      </div>
      <div
        className="conversation-list"
        onScroll={(e) => {
          const el = e.currentTarget;
          if (el.scrollHeight - el.scrollTop - el.clientHeight < 100) loadMore();
        }}
      >
        <p className="list-label">내 대화</p>
        {query.isLoading ? (
          <div className="skeletons">
            {[1, 2, 3, 4].map((i) => (
              <div key={i} />
            ))}
          </div>
        ) : (
          <>
            {query.error && (
              <div className="sidebar-state" role="alert">
                {query.error}
                <button onClick={() => void query.refresh()}>다시 시도</button>
              </div>
            )}
            {!items.length && (
              <p className="sidebar-state">
                {search ? '검색 결과가 없습니다.' : '아직 대화가 없습니다.'}
              </p>
            )}
            {items.map((c, i) => (
              <div
                key={c.id}
                className={`conversation ${active === c.id ? 'selected' : ''} ${i > 0 && items[i - 1].isPinned && !c.isPinned ? 'after-pinned' : ''}`}
              >
                <button
                  className="conversation-select"
                  onClick={() => {
                    onSelect(c.id);
                    setMenu(null);
                  }}
                  aria-current={active === c.id ? 'page' : undefined}
                >
                  {c.isPinned ? <Pin size={13} /> : <MessageSquare size={14} />}
                  <span>{c.title}</span>
                </button>
                <div className="conversation-menu-anchor">
                  <button
                    className="icon-button more"
                    aria-label={`${c.title} 메뉴`}
                    aria-expanded={menu === c.id}
                    onClick={() => setMenu(menu === c.id ? null : c.id)}
                  >
                    <MoreHorizontal size={17} />
                  </button>
                  {menu === c.id && (
                    <div
                      className="context-menu"
                      onKeyDown={(e) => {
                        if (e.key === 'Escape') {
                          setMenu(null);
                          (
                            e.currentTarget.parentElement?.querySelector('button') as HTMLElement
                          )?.focus();
                        }
                      }}
                    >
                      <button
                        onClick={() => {
                          setMenu(null);
                          onRename(c);
                        }}
                      >
                        <Pencil size={14} />
                        이름 변경
                      </button>
                      <button
                        disabled={busy}
                        onClick={() => {
                          setMenu(null);
                          onUpdate(c, 'pin');
                        }}
                      >
                        {c.isPinned ? <PinOff size={14} /> : <Pin size={14} />}
                        {c.isPinned ? '고정 해제' : '고정'}
                      </button>
                      <button
                        className="danger-text"
                        disabled={busy}
                        onClick={() => {
                          setMenu(null);
                          onUpdate(c, 'delete');
                        }}
                      >
                        <Trash2 size={14} />
                        삭제
                      </button>
                    </div>
                  )}
                </div>
              </div>
            ))}
            {query.hasMore && (
              <button className="load-more" disabled={query.isLoadingMore} onClick={loadMore}>
                {query.isLoadingMore ? '불러오는 중…' : '대화 더 보기'}
              </button>
            )}
            <div ref={sentinel} />
          </>
        )}
      </div>
      <div className="sidebar-bottom">
        <ProfileMenu profile={profile} onSettings={onSettings} onTrash={onTrash} />
      </div>
    </aside>
  );
}
