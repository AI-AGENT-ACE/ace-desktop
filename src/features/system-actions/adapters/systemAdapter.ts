import { invoke, isTauri } from '@tauri-apps/api/core';
import type {
  IpcRiskLevel,
  SystemActionRequest,
  SystemActionResult,
  ToolCall,
} from '../../../types';
const policies: Record<string, IpcRiskLevel> = {
  'system.status': 'SAFE',
  'app.open': 'SAFE',
  'app.close': 'CONFIRM',
  'app.focus': 'SAFE',
  'app.minimize': 'SAFE',
  'app.maximize': 'SAFE',
  'file.search': 'SAFE',
  'file.open': 'SAFE',
  'file.rename': 'CONFIRM',
  'file.move': 'CONFIRM',
  'file.copy': 'CONFIRM',
  'file.delete': 'CONFIRM',
  'folder.open': 'SAFE',
  'folder.search': 'SAFE',
  'folder.create': 'CONFIRM',
  'web.open': 'SAFE',
  'web.search': 'SAFE',
  'clipboard.write': 'SAFE',
  'clipboard.read': 'CONFIRM',
  'voice.activate': 'SAFE',
  'screen.capture': 'SAFE',
  'system.volume.set': 'SAFE',
  'system.volume.mute': 'SAFE',
  'system.open_settings': 'SAFE',
  'system.lock': 'CONFIRM',
  'system.shutdown': 'BLOCKED',
  'system.restart': 'BLOCKED',
  'shell.exec': 'BLOCKED',
  unsupported: 'BLOCKED',
};
const blockedAppAliases = /^(cmd|명령 프롬프트|powershell|파워쉘|pwsh|wscript|cscript|mshta)$/i;
const nativeErrors: Record<string, string> = {
  APP_NOT_FOUND: '설치된 앱 목록에서 요청한 앱을 찾지 못했습니다.',
  AMBIGUOUS_APP: '같은 별칭을 사용하는 앱이 여러 개입니다.',
  STALE_APP_PATH: '앱 경로가 바뀌었습니다. ACE를 다시 시작해 주세요.',
  DUPLICATE_REQUEST: '동일한 실행 요청이 이미 처리 중입니다.',
  ELEVATION_REQUIRED: '관리자 권한이 필요한 앱은 자동 실행할 수 없습니다.',
  EXECUTION_FAILED: '앱 실행 요청을 완료하지 못했습니다.',
  APP_LAUNCH_FAILED: '앱을 실행하지 못했습니다.',
  WINDOW_NOT_FOUND: '실행 중인 앱 창을 찾지 못했습니다.',
  FILE_NOT_FOUND: '파일을 찾지 못했습니다.',
  DIRECTORY_NOT_FOUND: '폴더를 찾지 못했습니다.',
  INVALID_ARGUMENT: '요청 인자가 올바르지 않습니다.',
  PATH_NOT_ALLOWED: '허용된 사용자 폴더 밖에는 접근할 수 없습니다.',
  DESTINATION_EXISTS: '대상 위치에 같은 이름이 이미 있습니다.',
  PERMISSION_DENIED: '운영체제 권한이 거부되었습니다.',
  CLIPBOARD_DENIED: '클립보드에 접근할 수 없습니다.',
  SCREEN_CAPTURE_FAILED: '화면을 캡처하지 못했습니다.',
  BLOCKED_BY_POLICY: 'ACE 보안 정책으로 차단된 기능입니다.',
  AMBIGUOUS_APP_MATCH: '여러 앱이 비슷하게 일치합니다. 실행할 앱을 다시 지정해 주세요.',
  AMBIGUOUS_FILE_MATCH: '여러 파일이 일치합니다. 검색 결과에서 대상을 선택해 주세요.',
  NO_MATCH_FOUND: '일치하는 대상을 찾지 못했습니다.',
  INVALID_CANDIDATES: '대상 후보 정보가 올바르지 않습니다.',
  RESOURCE_EXPIRED: '검색 결과의 사용 시간이 만료되었습니다. 다시 검색해 주세요.',
  RESOURCE_NOT_FOUND: '검색 결과를 찾지 못했습니다. 다시 검색해 주세요.',
};
interface NativeToolResult {
  success: boolean;
  tool: string;
  data: Record<string, unknown> | null;
  error: { code: string; message: string } | null;
}
export function localPolicy(commandType: string): IpcRiskLevel {
  return policies[commandType] || 'BLOCKED';
}
export function toolLabel(tool: string, args: Record<string, unknown> = {}): string {
  const names: Record<string, string> = {
    'app.open': '앱 실행',
    'app.close': '앱 종료',
    'file.open': '파일 열기',
    'file.delete': '파일 삭제',
    'file.rename': '파일 이름 변경',
    'file.search': '파일 찾기',
    'folder.open': '폴더 열기',
  };
  const target = args.original ?? args.appName ?? args.canonicalId ?? args.path;
  return `${names[tool] || tool}${typeof target === 'string' ? ` · ${target.slice(0, 80)}` : ''}`;
}
function objectParticle(value: string): string {
  const characters = Array.from(value.trim());
  const last = characters[characters.length - 1];
  const code = last?.charCodeAt(0) ?? 0;
  return code >= 0xac00 && code <= 0xd7a3 && (code - 0xac00) % 28 !== 0 ? '을' : '를';
}
export function confirmationPrompt(request: SystemActionRequest): string {
  const args = request.arguments;
  const rawTarget = args.appName ?? args.original ?? args.canonicalId ?? args.path;
  const target = typeof rawTarget === 'string' ? rawTarget.trim() : '';
  if (target && request.commandType === 'app.open')
    return `${target}${objectParticle(target)} 실행할까요?`;
  if (target && request.commandType === 'app.close')
    return `${target}${objectParticle(target)} 종료할까요?`;
  if (target && request.commandType === 'file.delete')
    return `${target}${objectParticle(target)} 삭제할까요?`;
  if (request.commandType === 'system.lock') return '화면을 잠글까요?';
  return `${request.label} 작업을 진행할까요?`;
}
export function toolRequest(call: ToolCall): SystemActionRequest {
  return {
    contractVersion: call.version || '1.1',
    commandType: call.tool,
    arguments: call.arguments,
    label: toolLabel(call.tool, call.arguments),
    riskLevel: localPolicy(call.tool),
  };
}
// Voice parser is a strict allowlist, not an arbitrary shell interpreter.
export function classifyVoiceCommand(text: string): SystemActionRequest {
  if (
    /^(?:바탕\s*화면에서\s*)?(?:김환성[_\s]*)?포트폴리오(?:\.pdf)?\s*(?:열어줘|열어\s*줘)$/i.test(
      text.trim(),
    )
  )
    return {
      contractVersion: '1.1',
      commandType: 'file.open',
      arguments: { directory: 'desktop', path: '김환성_포트폴리오.pdf' },
      label: '김환성 포트폴리오 열기',
      riskLevel: 'SAFE',
    };
  if (/^(시스템 상태( 조회)?|system status)$/i.test(text))
    return {
      commandType: 'system.status',
      arguments: {},
      label: '시스템 상태 조회',
      riskLevel: 'SAFE',
    };
  const match = /^(.{1,80}?)\s*(열어줘|실행|켜줘|종료|닫아줘)$/.exec(text.trim());
  if (match) {
    const target = match[1].trim();
    if (blockedAppAliases.test(target))
      return {
        commandType: 'unsupported',
        arguments: {},
        label: 'Shell 실행 차단',
        riskLevel: 'BLOCKED',
      };
    const commandType = /종료|닫아줘/.test(match[2]) ? 'app.close' : 'app.open';
    return {
      commandType,
      arguments: { original: target, candidates: [target] },
      label: `${target} ${commandType === 'app.open' ? '실행' : '종료'}`,
      riskLevel: localPolicy(commandType),
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
    const native = await invoke<NativeToolResult>('execute_native_tool', {
      version: request.contractVersion || '1.1',
      tool: request.commandType,
      arguments: request.arguments,
      confirmed,
    });
    if (!native.success) {
      const errorCode = native.error?.code || 'EXECUTION_FAILED';
      return {
        success: false,
        errorCode,
        message: native.error?.message || nativeErrors[errorCode],
      };
    }
    return {
      success: true,
      data: native.data ?? undefined,
      message: native.data?.message as string | undefined,
    };
  },
};
