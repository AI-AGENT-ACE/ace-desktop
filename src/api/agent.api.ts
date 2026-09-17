import { apiClient } from './client';
import type { AgentTurn, ToolCall } from '../types';
export const agentApi = {
  async turn(conversationId: string, content: string) {
    return (
      await apiClient.post<AgentTurn>(
        '/agent/turns',
        { conversationId, content },
        { timeout: 30000 },
      )
    ).data;
  },
  async cloud(call: ToolCall, confirmed: boolean) {
    return (
      await apiClient.post<AgentTurn>(
        '/agent/cloud-tools',
        { ticket: call.ticket, arguments: call.arguments, confirmed },
        { timeout: 30000 },
      )
    ).data;
  },
  async local(
    call: ToolCall,
    status: 'SUCCEEDED' | 'FAILED' | 'DENIED',
    confirmed: boolean,
    duration: number,
    result?: { summary: string },
  ) {
    return (
      await apiClient.post<AgentTurn>(
        '/agent/tool-results',
        { ticket: call.ticket, status, confirmed, duration, ...(result ? { result } : {}) },
        { timeout: 30000 },
      )
    ).data;
  },
  async health(signal?: AbortSignal) {
    return (
      await apiClient.get<{
        ai: 'configured' | 'not_configured';
        weather: 'configured' | 'not_configured';
      }>('/health', { signal, skipAuth: true })
    ).data;
  },
};
