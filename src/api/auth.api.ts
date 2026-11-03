import { apiClient } from './client';
import type { AuthResponse, User } from '../types';
export const authApi = {
  async updateProfile(displayName: string) {
    const user = (await apiClient.patch<User>('/users/me', { displayName })).data;
    window.dispatchEvent(new CustomEvent('ace-profile-updated', { detail: user }));
    return user;
  },
  async login(email: string, password: string) {
    return (
      await apiClient.post<AuthResponse>('/auth/login', { email, password }, { skipAuth: true })
    ).data;
  },
  async register(email: string, password: string, displayName: string) {
    return (
      await apiClient.post<AuthResponse>(
        '/auth/register',
        { email, password, displayName: displayName.trim() },
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
