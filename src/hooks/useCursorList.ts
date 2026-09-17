import { useCallback, useEffect, useRef, useState } from 'react';
import { apiErrorMessage, isCancelled } from '../api/client';
import type { Page } from '../types';
type Loader<T> = (cursor: string | null, signal: AbortSignal) => Promise<Page<T>>;
export function useCursorList<T extends { id: string }>(
  loader: Loader<T>,
  enabled = true,
  prepend = false,
) {
  const [items, setItems] = useState<T[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [isLoading, setLoading] = useState(enabled);
  const [isLoadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const current = useRef({ cursor: null as string | null, hasMore: false });
  const flight = useRef<AbortController | null>(null);
  const epoch = useRef(0);
  const request = useCallback(
    async (more: boolean) => {
      if (!enabled || (more && (flight.current || !current.current.hasMore))) return;
      if (!more) flight.current?.abort();
      const version = ++epoch.current;
      const controller = new AbortController();
      flight.current = controller;
      if (more) setLoadingMore(true);
      else {
        setLoading(true);
        setLoadingMore(false);
      }
      setError(null);
      try {
        const page = await loader(more ? current.current.cursor : null, controller.signal);
        if (controller.signal.aborted || version !== epoch.current) return;
        setItems((previous) => {
          if (!more) return page.items;
          const existing = new Set(previous.map((item) => item.id));
          const added = page.items.filter((item) => !existing.has(item.id));
          return prepend ? [...added, ...previous] : [...previous, ...added];
        });
        current.current = { cursor: page.nextCursor, hasMore: page.hasMore };
        setNextCursor(page.nextCursor);
        setHasMore(page.hasMore);
      } catch (cause) {
        if (!controller.signal.aborted && version === epoch.current && !isCancelled(cause))
          setError(apiErrorMessage(cause));
      } finally {
        if (version === epoch.current) {
          flight.current = null;
          setLoading(false);
          setLoadingMore(false);
        }
      }
    },
    [enabled, loader, prepend],
  );
  useEffect(() => {
    setItems([]);
    current.current = { cursor: null, hasMore: false };
    setHasMore(false);
    setNextCursor(null);
    setError(null);
    if (enabled) void request(false);
    else {
      setLoading(false);
      setLoadingMore(false);
    }
    return () => {
      ++epoch.current;
      flight.current?.abort();
      flight.current = null;
    };
  }, [enabled, request]);
  const refresh = useCallback(() => request(false), [request]);
  const loadMore = useCallback(() => request(true), [request]);
  const remove = useCallback(
    (id: string) => {
      setItems((previous) => previous.filter((item) => item.id !== id));
      // Moving/deleting the boundary item invalidates the server's list-scoped cursor.
      if (current.current.hasMore && current.current.cursor === id) void request(false);
    },
    [request],
  );
  const upsert = useCallback(
    (item: T) => setItems((previous) => [item, ...previous.filter((old) => old.id !== item.id)]),
    [],
  );
  return {
    items,
    nextCursor,
    hasMore,
    isLoading,
    isLoadingMore,
    error,
    refresh,
    loadMore,
    remove,
    upsert,
    setItems,
  };
}
