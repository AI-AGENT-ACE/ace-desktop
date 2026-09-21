import type { TokenPair } from '../types';
const key = 'ace-auth-session';
let pair: TokenPair | null = null;
let persistent = false;
function read(storage: Storage) {
  try {
    const saved: unknown = JSON.parse(storage.getItem(key) || 'null');
    if (
      saved &&
      typeof saved === 'object' &&
      'accessToken' in saved &&
      'refreshToken' in saved &&
      typeof saved.accessToken === 'string' &&
      typeof saved.refreshToken === 'string'
    )
      return saved as TokenPair;
  } catch {
    storage.removeItem(key);
  }
  return null;
}
pair = read(localStorage);
if (pair) persistent = true;
else pair = read(sessionStorage);
export const getSession = () => pair;
export const isPersistentSession = () => persistent;
export function setSession(next: TokenPair | null, remember = persistent) {
  pair = next;
  persistent = !!next && remember;
  localStorage.removeItem(key);
  sessionStorage.removeItem(key);
  if (next) (persistent ? localStorage : sessionStorage).setItem(key, JSON.stringify(next));
  window.dispatchEvent(new Event('ace-session-change'));
}
