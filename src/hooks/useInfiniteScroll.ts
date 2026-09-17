import { useEffect, useRef } from 'react';
export function useInfiniteScroll(loadMore: () => Promise<void>, enabled: boolean) {
  const sentinel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const element = sentinel.current;
    if (!element || !enabled) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) void loadMore();
      },
      { root: element.parentElement, rootMargin: '80px' },
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, [enabled, loadMore]);
  return sentinel;
}
