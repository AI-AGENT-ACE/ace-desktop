import { useCallback } from 'react';
import { messagesApi } from '../api/messages.api';
import type { Message, Page } from '../types';
import { useCursorList } from './useCursorList';
export function useMessages(id: string | null) {
  const loader = useCallback(
    async (cursor: string | null, signal: AbortSignal): Promise<Page<Message>> => {
      if (!id) return { items: [], nextCursor: null, hasMore: false };
      const page = await messagesApi.list(id, cursor, signal);
      return { ...page, items: page.items.slice().reverse() };
    },
    [id],
  );
  const query = useCursorList(loader, !!id, true);
  const append = useCallback(
    (message: Message) => {
      if (message.conversationId !== id) return;
      query.setItems((previous) => [...previous.filter((item) => item.id !== message.id), message]);
    },
    [id, query.setItems],
  );
  return { ...query, append };
}
