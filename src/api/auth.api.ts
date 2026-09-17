import { apiClient } from './client';
import type { AuthResponse, User } from '../types';
export const authApi = {
  async login(email: string, password: string) {
    return (
      await apiClient.post<AuthResponse>('/auth/login', { email, password }, { skipAuth: true })
    ).data;
  },
  async register(email: string, password: string, displayName: string) {
    return (
      await apiClient.post<AuthResponse>(
        '/auth/register',
        { email, password, ...(displayName.trim() ? { displayName: displayName.trim() } : {}) },
        { skipAuth: true },
      )
    ).data;
  },
  async me(signal?: AbortSignal) {
    return (await apiClient.get<User>('/users/me', { signal })).data;
  },
  async logout() {
    await apiClient.post('/auth/logout');
  },
};
