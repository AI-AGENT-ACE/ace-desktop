import { loadEnv } from 'vite';
import { run } from '@tauri-apps/cli';
const args = process.argv.slice(2);
if (['dev', 'build'].includes(args[0])) {
  const env = loadEnv(args[0] === 'dev' ? 'development' : 'production', process.cwd(), 'VITE_');
  const url = new URL(process.env.VITE_API_BASE_URL || env.VITE_API_BASE_URL || '');
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password)
    throw new Error('Invalid VITE_API_BASE_URL');
  const connect = ["'self'", 'ipc:', 'http://ipc.localhost', url.origin];
  if (args[0] === 'dev')
    connect.push('ws://localhost:1420', 'ws://127.0.0.1:1420', 'ws://localhost:1421');
  const scripts = args[0] === 'dev' ? "'self' 'unsafe-inline'" : "'self'";
  const csp = `default-src 'self'; script-src ${scripts}; style-src 'self' 'unsafe-inline'; img-src 'self' asset: http://asset.localhost data:; font-src 'self' data:; connect-src ${connect.join(' ')}`;
  args.push('--config', JSON.stringify({ app: { security: { csp } } }));
}
await run(args);
