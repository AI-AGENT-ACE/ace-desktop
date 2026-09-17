import { conversationApi } from '../api/conversations.api';
import { useCursorList } from './useCursorList';
export function useConversations() {
  return useCursorList(conversationApi.list);
}
export function useTrashConversations() {
  return useCursorList(conversationApi.trash);
}
