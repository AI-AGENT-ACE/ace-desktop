import React, { useState } from 'react';
import { createRoot } from 'react-dom/client';
import { ChatComposer } from '../src/features/chat/ChatComposer';
import { MessageList } from '../src/features/chat/MessageList';
import '../src/styles/globals.css';
import '../src/styles/chat-experience.css';
import type { Message } from '../src/types';

function Harness() {
  const [sent, setSent] = useState<Message | null>(null);
  const [history, setHistory] = useState(['첫 번째 명령', '두 번째 명령', '세 번째 명령']);
  const [loading, setLoading] = useState(false);
  const items = Array.from({ length: 40 }, (_, i) => ({
    id: `m${i}`,
    conversationId: 'test',
    role: 'USER',
    content: `과거 메시지 ${i}`,
    createdAt: new Date().toISOString(),
  })) as Message[];
  return (
    <div style={{ height: 550, display: 'flex', flexDirection: 'column' }}>
      <MessageList
        query={
          { items, isLoading: false, isLoadingMore: false, hasMore: false, error: null } as never
        }
        sending={false}
        optimistic={sent}
      />
      <ChatComposer
        history={history}
        hasOlderHistory={history.length === 3}
        historyLoading={loading}
        onLoadOlderHistory={async () => {
          setLoading(true);
          await new Promise((resolve) => setTimeout(resolve, 20));
          setHistory(['이전 페이지 명령', ...history]);
          setLoading(false);
        }}
        busy={false}
        onVoice={() => {}}
        onSend={async (text) => {
          setSent({
            id: crypto.randomUUID(),
            conversationId: 'test',
            role: 'USER',
            content: text,
            createdAt: new Date().toISOString(),
          } as Message);
          return true;
        }}
      />
    </div>
  );
}
export function mount() {
  const host = document.createElement('div');
  document.body.replaceChildren(host);
  createRoot(host).render(<Harness />);
}
