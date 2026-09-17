import type { TokenPair } from '../types';
const key = 'ace-auth-session';
let pair: TokenPair | null = null;
try {
  const saved: unknown = JSON.parse(sessionStorage.getItem(key) || 'null');
  if (
    saved &&
    typeof saved === 'object' &&
    'accessToken' in saved &&
    'refreshToken' in saved &&
    typeof saved.accessToken === 'string' &&
    typeof saved.refreshToken === 'string'
  )
    pair = saved as TokenPair;
} catch {
  sessionStorage.removeItem(key);
}
export const getSession = () => pair;
export function setSession(next: TokenPair | null) {
  pair = next;
  if (next) sessionStorage.setItem(key, JSON.stringify(next));
  else sessionStorage.removeItem(key);
  window.dispatchEvent(new Event('ace-session-change'));
}
