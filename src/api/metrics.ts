import type { AxiosRequestConfig } from 'axios';

interface RequestMetric {
  method: string;
  route: string;
  status: number | null;
  durationMs: number;
  outcome: 'completed' | 'cancelled' | 'network-error';
}
const entries: RequestMetric[] = [];
const staticRoutes = new Set([
  '/health',
  '/auth/login',
  '/auth/register',
  '/auth/refresh',
  '/auth/logout',
  '/users/me',
  '/conversations',
  '/conversations/trash',
  '/settings',
  '/settings/permissions',
  '/tools',
  '/agent/turns',
  '/agent/cloud-tools',
  '/agent/tool-results',
  '/logs/voice',
]);
function routeName(url = '') {
  const path = url.split('?')[0];
  if (staticRoutes.has(path)) return path;
  if (/^\/conversations\/[a-z0-9]{20,40}(\/(messages|restore|permanent))?$/.test(path))
    return path.replace(/^\/conversations\/[^/]+/, '/conversations/:id');
  if (/^\/settings\/permissions\/[a-z.]+$/.test(path)) return '/settings/permissions/:tool';
  return 'OTHER';
}
declare global {
  interface Window {
    aceApiMetrics?: { snapshot: () => RequestMetric[]; clear: () => void };
  }
}
if (import.meta.env.DEV)
  window.aceApiMetrics = {
    snapshot: () => entries.map((entry) => ({ ...entry })),
    clear: () => {
      entries.length = 0;
    },
  };
export function recordRequestMetric(
  config: AxiosRequestConfig | undefined,
  status: number | null,
  cancelled = false,
) {
  if (!import.meta.env.DEV || !config || config.baselineStartedAt === undefined) return;
  // Metadata only: no credentials, bodies, arguments, IDs, query strings or error text.
  entries.push({
    method: (config.method || 'get').toUpperCase(),
    route: routeName(config.url),
    status,
    durationMs: Math.round((performance.now() - config.baselineStartedAt) * 10) / 10,
    outcome: cancelled ? 'cancelled' : status === null ? 'network-error' : 'completed',
  });
  if (entries.length > 500) entries.shift();
}
