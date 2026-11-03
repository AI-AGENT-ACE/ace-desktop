import { invoke, isTauri } from '@tauri-apps/api/core';
import { agentApi } from '../../../api/agent.api';
import { toolPresentation } from '../toolPresentation';
import type { ApprovalView } from '../TargetDetails';
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
  'clipboard.write': 'CONFIRM',
  'clipboard.read': 'SAFE',
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
  WINDOW_SELECTION_REQUIRED: '작업할 창을 선택해 주세요.',
  WINDOW_TARGET_STALE: '선택한 창이 닫혔거나 대상이 변경되었습니다. 다시 선택해 주세요.',
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
  WINDOW_FOCUS_FAILED:
    '대상 창을 앞으로 가져오지 못해 종료 요청을 보내지 않았습니다.\n창을 직접 확인한 뒤 다시 시도해 주세요.',
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
export function normalizeAppArguments(
  tool: string,
  args: Record<string, unknown>,
): Record<string, unknown> {
  if (!['app.open', 'app.close', 'app.focus', 'app.minimize', 'app.maximize'].includes(tool))
    return args;
  const allowed = ['appName', 'appId', 'canonicalId', 'original', 'candidates', 'targetWindowId'];
  if (Object.keys(args).some((key) => !allowed.includes(key))) return args;
  // Match the backend text adapter: the AI's canonical app ID takes precedence over its wording.
  const appName = [args.appName, args.canonicalId, args.appId, args.original].find(
    (value) => typeof value === 'string' && value.trim().length > 0,
  );
  return typeof appName === 'string'
    ? {
        appName: appName.trim(),
        ...(args.targetWindowId ? { targetWindowId: args.targetWindowId } : {}),
      }
    : args;
}
export function toolLabel(tool: string, args: Record<string, unknown> = {}): string {
  const target = args.original ?? args.appName ?? args.canonicalId ?? args.path;
  return `${toolPresentation(tool).name}${typeof target === 'string' ? ` · ${target.slice(0, 80)}` : ''}`;
}
function objectParticle(value: string): string {
  const characters = Array.from(value.trim());
  const last = characters[characters.length - 1];
  const code = last?.charCodeAt(0) ?? 0;
  return code >= 0xac00 && code <= 0xd7a3 && (code - 0xac00) % 28 !== 0 ? '을' : '를';
}
export function confirmationPrompt(request: SystemActionRequest): string {
  if (request.commandType === 'clipboard.read') return '복사한 텍스트를 읽을까요?';
  const args = request.arguments;
  const rawTarget = args.appName ?? args.original ?? args.canonicalId ?? args.path;
  const target = typeof rawTarget === 'string' ? rawTarget.trim() : '';
  if (target && target.length <= 24 && request.commandType === 'app.open')
    return `${target}${objectParticle(target)} 열까요?`;
  const questions: Record<string, string> = {
    'app.open': '이 앱을 열까요?',
    'app.close': '이 창을 닫을까요?',
    'app.focus': '이 창을 앞으로 가져올까요?',
    'app.minimize': '이 창을 최소화할까요?',
    'app.maximize': '이 창을 최대화할까요?',
    'file.open': '이 파일을 열까요?',
    'file.delete': '이 파일을 휴지통으로 옮길까요?',
    'file.copy': '이 파일을 복사할까요?',
    'file.move': '이 파일을 이동할까요?',
    'file.rename': '이 파일의 이름을 바꿀까요?',
    'folder.open': '이 폴더를 열까요?',
    'folder.create': '새 폴더를 만들까요?',
    'screen.capture': '선택한 화면을 캡처할까요?',
    'system.lock': '컴퓨터를 잠글까요?',
    'web.open': '이 웹사이트를 열까요?',
    'web.search': '인터넷에서 검색할까요?',
  };
  return questions[request.commandType] || '이 작업을 진행할까요?';
}
export function confirmationView(request: SystemActionRequest): ApprovalView {
  const actions: Record<string, string> = {
    'app.open': '열기',
    'app.close': '닫기',
    'app.focus': '앞으로 가져오기',
    'app.minimize': '최소화',
    'app.maximize': '최대화',
    'file.open': '열기',
    'file.copy': '복사',
    'file.move': '이동',
    'file.rename': '이름 바꾸기',
    'file.delete': '휴지통으로 이동',
    'folder.open': '열기',
    'folder.create': '만들기',
    'screen.capture': '캡처',
    'system.lock': '잠금',
    'web.open': '열기',
    'web.search': '검색',
    'clipboard.read': '읽기',
    'clipboard.write': '복사',
  };
  const value = request.arguments.appName ?? request.arguments.original ?? request.arguments.url;
  return {
    question: confirmationPrompt(request),
    action: actions[request.commandType] || '실행',
    targets: typeof value === 'string' ? [{ label: '대상', name: value }] : [],
    warning: request.commandType === 'file.delete' ? '휴지통에서 복원할 수 있습니다.' : undefined,
  };
}
export function toolRequest(call: ToolCall): SystemActionRequest {
  return {
    requestId: call.requestId || call.id,
    executionId: call.executionId || call.id,
    executionTicket: call.ticket,
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
let activeCapture: string | null = null;
export const systemAdapter = {
  async cancelPendingCapture() {
    if (activeCapture)
      await invoke('cancel_screen_capture', { sessionId: activeCapture }).catch(() => undefined);
  },
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
    if (
      ['file.open', 'file.copy', 'file.move', 'file.rename', 'file.delete'].includes(
        request.commandType,
      ) &&
      request.arguments.query &&
      !request.arguments.resourceId &&
      !request.arguments.path &&
      !request.arguments.sourcePath
    ) {
      return {
        success: false,
        errorCode: 'FILE_TARGET_SELECTION_REQUIRED',
        data: request.arguments,
        message: '실행할 파일과 대상 위치를 확인해 주세요.',
      };
    }
    const arguments_ = normalizeAppArguments(request.commandType, request.arguments);
    request.executionId ||= crypto.randomUUID();
    request.attemptId ||= crypto.randomUUID();
    if (risk === 'CONFIRM' && !request.approvalId) {
      const prepared = await invoke<NativeToolResult>('prepare_native_tool', {
        tool: request.commandType,
        arguments: arguments_,
        executionId: request.executionId,
        attemptId: request.attemptId,
      });
      if (prepared.error?.code === 'NATIVE_APPROVAL_REQUIRED') {
        if (request.commandType === 'app.close' && prepared.data?.preparedArguments)
          request.arguments = prepared.data.preparedArguments as Record<string, unknown>;
        await agentApi.executionEvent(request, 'WAITING_APPROVAL');
      }
      return {
        success: false,
        errorCode: prepared.error?.code || 'NATIVE_APPROVAL_REQUIRED',
        data: prepared.data || undefined,
        message: prepared.error?.message,
      };
    }
    await agentApi.executionEvent(request, 'RUNNING');
    const native = await invoke<NativeToolResult>('execute_native_tool', {
      version: request.contractVersion || '1.1',
      tool: request.commandType,
      arguments: arguments_,
      confirmed,
      approvalId: request.approvalId,
      executionId: request.executionId,
      attemptId: request.attemptId,
      requestId: request.requestId,
    });
    if (!native.success) {
      const errorCode = native.error?.code || 'EXECUTION_FAILED';
      return {
        success: false,
        errorCode,
        data: native.data ?? undefined,
        message: native.error?.message || nativeErrors[errorCode],
      };
    }
    if (typeof native.data?.closeTicket === 'string') {
      const observed = await invoke<{ closed: boolean; message: string }>('observe_window_close', {
        ticket: native.data.closeTicket,
      });
      return {
        success: true,
        data: { ...native.data, closed: observed.closed },
        message: observed.message,
      };
    }
    if (typeof native.data?.captureSessionId === 'string') {
      activeCapture = native.data.captureSessionId;
      try {
        const data = await invoke<{ resourceId: string; displayName: string }>(
          'await_screen_capture',
          { sessionId: activeCapture },
        );
        return {
          success: true,
          data: { ...data, captured: true },
          message: '화면 캡처를 완료했습니다.',
        };
      } catch (cause) {
        return {
          success: false,
          errorCode: String(cause),
          message:
            String(cause) === 'CAPTURE_MONITOR_DISCONNECTED'
              ? '선택한 모니터의 연결 또는 배치가 변경되었습니다.'
              : String(cause) === 'CAPTURE_CANCELLED'
                ? '화면 캡처를 취소했습니다.'
                : '화면 캡처에 실패했습니다.',
        };
      } finally {
        activeCapture = null;
      }
    }
    if (native.data?.activationRequested) {
      try {
        const ready = await invoke<{ state: string }>('await_voice_activation');
        return {
          success: true,
          data: { ...native.data, ...ready },
          message: '음성 입력을 시작했습니다.',
        };
      } catch (cause) {
        return {
          success: false,
          errorCode: String(cause),
          message: '음성 입력을 시작하지 못했습니다. 마이크와 Orb 상태를 확인해 주세요.',
        };
      }
    }
    return {
      success: true,
      data: native.data ?? undefined,
      message: native.data?.message as string | undefined,
    };
  },
};
