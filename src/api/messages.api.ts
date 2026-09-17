import { apiClient } from './client';
import type { Message, Page } from '../types';
export const messagesApi = {
  async list(id: string, cursor?: string | null, signal?: AbortSignal) {
    return (
      await apiClient.get<Page<Message>>(`/conversations/${id}/messages`, {
        params: { limit: 30, cursor: cursor || undefined },
        signal,
      })
    ).data;
  },
  async create(id: string, content: string) {
    return (
      await apiClient.post<Message>(`/conversations/${id}/messages`, { content, role: 'USER' })
    ).data;
  },
};
