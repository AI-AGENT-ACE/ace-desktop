import { apiClient } from './client';
import type { Conversation, Page, TrashConversation } from '../types';
export const conversationApi = {
  async list(cursor?: string | null, signal?: AbortSignal) {
    return (
      await apiClient.get<Page<Conversation>>('/conversations', {
        params: { limit: 20, cursor: cursor || undefined },
        signal,
      })
    ).data;
  },
  async trash(cursor?: string | null, signal?: AbortSignal) {
    return (
      await apiClient.get<Page<TrashConversation>>('/conversations/trash', {
        params: { limit: 20, cursor: cursor || undefined },
        signal,
      })
    ).data;
  },
  async create() {
    return (await apiClient.post<Conversation>('/conversations', {})).data;
  },
  async get(id: string, signal?: AbortSignal) {
    return (await apiClient.get<Conversation>(`/conversations/${id}`, { signal })).data;
  },
  async update(id: string, input: { title?: string; isPinned?: boolean }) {
    return (await apiClient.patch<Conversation>(`/conversations/${id}`, input)).data;
  },
  async remove(id: string) {
    await apiClient.delete(`/conversations/${id}`);
  },
  async restore(id: string) {
    return (await apiClient.patch<Conversation>(`/conversations/${id}/restore`)).data;
  },
  async permanentlyDelete(id: string) {
    await apiClient.delete(`/conversations/${id}/permanent`);
  },
};
