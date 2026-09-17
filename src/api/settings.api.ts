import { apiClient } from './client';
import type { AgentSettings, Permission, PermissionPolicy } from '../types';
export const settingsApi = {
  async get(signal?: AbortSignal) {
    return (await apiClient.get<AgentSettings>('/settings', { signal })).data;
  },
  async update(input: AgentSettings) {
    return (
      await apiClient.patch<AgentSettings>('/settings', {
        responseLanguage: input.responseLanguage,
        ttsEnabled: input.ttsEnabled,
      })
    ).data;
  },
  async permissions(signal?: AbortSignal): Promise<Permission[]> {
    const [preferences, catalog] = await Promise.all([
      apiClient.get<{ toolName: string; policy: PermissionPolicy }[]>('/settings/permissions', {
        signal,
      }),
      apiClient.get<
        { name: string; defaultPolicy: PermissionPolicy; systemConfirmation: boolean }[]
      >('/tools', { signal }),
    ]);
    return catalog.data.map((tool) => {
      const policy =
        preferences.data.find((item) => item.toolName === tool.name)?.policy || tool.defaultPolicy;
      return {
        toolName: tool.name,
        policy,
        systemConfirmation: tool.systemConfirmation,
        requiresConfirmation: tool.systemConfirmation || policy !== 'ALWAYS_ALLOW',
      };
    });
  },
  async permission(tool: string, policy: PermissionPolicy) {
    return (await apiClient.put<Permission>(`/settings/permissions/${tool}`, { policy })).data;
  },
};
