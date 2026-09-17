import { invoke, isTauri } from '@tauri-apps/api/core';
import type {
  IpcRiskLevel,
  SystemActionRequest,
  SystemActionResult,
  ToolCall,
} from '../../../types';
const policies: Record<string, IpcRiskLevel> = {
  'system.status': 'SAFE',
  'app.open': 'CONFIRM',
  'app.close': 'CONFIRM',
  'file.open': 'BLOCKED',
  'file.rename': 'BLOCKED',
  'file.delete': 'BLOCKED',
  unsupported: 'BLOCKED',
};
export function localPolicy(commandType: string): IpcRiskLevel {
  return policies[commandType] || 'BLOCKED';
}
export function toolRequest(call: ToolCall): SystemActionRequest {
  return {
    commandType: call.tool,
    arguments: call.arguments,
    label: call.tool,
    riskLevel: localPolicy(call.tool),
  };
}
// Voice parser is a strict allowlist, not an arbitrary shell interpreter.
export function classifyVoiceCommand(text: string): SystemActionRequest {
  if (/^(시스템 상태( 조회)?|system status)$/i.test(text))
    return {
      commandType: 'system.status',
      arguments: {},
      label: '시스템 상태 조회',
      riskLevel: 'SAFE',
    };
  const match = /^(메모장|계산기)\s*(열어줘|실행|켜줘|종료|닫아줘)$/.exec(text);
  if (match) {
    const commandType = /종료|닫아줘/.test(match[2]) ? 'app.close' : 'app.open';
    return {
      commandType,
      arguments: { appName: match[1] === '메모장' ? 'Notepad' : 'Calculator' },
      label: `${match[1]} ${commandType === 'app.open' ? '실행' : '종료'}`,
      riskLevel: 'CONFIRM',
    };
  }
  return {
    commandType: 'unsupported',
    arguments: {},
    label: '지원하지 않는 음성 명령',
    riskLevel: 'BLOCKED',
  };
}
export const systemAdapter = {
  async execute(request: SystemActionRequest, confirmed = false): Promise<SystemActionResult> {
    const risk = localPolicy(request.commandType);
    if (risk === 'BLOCKED') return { success: false, errorCode: 'BLOCKED' };
    if (risk === 'CONFIRM' && !confirmed)
      return { success: false, errorCode: 'CONFIRMATION_REQUIRED' };
    if (!isTauri())
      return {
        success: false,
        errorCode: 'DESKTOP_REQUIRED',
        message: '로컬 명령은 Tauri 데스크톱 앱에서 실행할 수 있습니다.',
      };
    return invoke<SystemActionResult>('execute_local_command', {
      commandType: request.commandType,
      arguments: request.arguments,
      confirmed,
    });
  },
};
