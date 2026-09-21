export interface User {
  id: string;
  email: string;
  displayName: string | null;
  createdAt: string;
  updatedAt: string;
}
export interface TokenPair {
  accessToken: string;
  refreshToken: string;
  tokenType: 'Bearer';
  expiresIn: number;
  refreshExpiresAt: string;
}
export interface AuthResponse extends TokenPair {
  user: User;
}
export interface Conversation {
  id: string;
  userId: string;
  title: string;
  isPinned: boolean;
  createdAt: string;
  updatedAt: string;
  deletedAt: string | null;
}
export interface TrashConversation extends Conversation {
  expiresAt: string;
  remainingDays: number;
  isRestorable: boolean;
}
export interface Message {
  id: string;
  conversationId: string;
  role: 'USER' | 'ASSISTANT' | 'TOOL' | 'SYSTEM';
  content: string;
  createdAt: string;
  attachments: Attachment[];
}
export interface Attachment {
  id: string;
  originalName: string;
  mimeType: string;
  size: number;
  createdAt: string;
}
export interface Page<T> {
  items: T[];
  nextCursor: string | null;
  hasMore: boolean;
}
export interface ApiErrorResponse {
  statusCode: number;
  code?: string;
  message: string | string[];
  requestId?: string;
}
export interface AgentSettings {
  responseLanguage: string;
  ttsEnabled: boolean;
  id: string;
  userId: string;
  createdAt: string;
  updatedAt: string;
}
export type PermissionPolicy = 'ALWAYS_ALLOW' | 'ASK' | 'ALWAYS_ASK';
export interface Permission {
  toolName: string;
  policy: PermissionPolicy;
  requiresConfirmation: boolean;
  systemConfirmation: boolean;
}
export type VoiceState = 'idle' | 'listening' | 'processing' | 'error';
export type SystemActionStatus = 'pending' | 'success' | 'error';
export type IpcRiskLevel = 'SAFE' | 'CONFIRM' | 'BLOCKED';
export interface SystemActionRequest {
  commandType: string;
  arguments: Record<string, unknown>;
  riskLevel: IpcRiskLevel;
  label: string;
}
export interface SystemActionResult {
  success: boolean;
  message?: string;
  errorCode?: string;
}
export interface VoiceLogInput {
  commandType: string;
  status: 'SUCCESS' | 'FAILED' | 'CANCELLED';
  duration: number;
  errorCode?: string;
}
export interface ToolCall {
  id: string;
  tool: string;
  arguments: Record<string, unknown>;
  executionLocation: 'LOCAL' | 'CLOUD';
  policy: PermissionPolicy;
  requiresConfirmation: boolean;
  systemConfirmation: boolean;
  ticket: string;
}
export interface AgentTurn {
  conversationId: string;
  message: Message | null;
  toolCalls: ToolCall[];
}
