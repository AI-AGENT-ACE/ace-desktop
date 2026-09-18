import { useRef, useState } from 'react';
import { ArrowRight, Eye, EyeOff, LoaderCircle } from 'lucide-react';
import type { AuthInput } from '../hooks/useAuth';

const passwordComposition =
  /^(?=.*[A-Za-z])(?=.*[0-9])(?=.*[\x21-\x2F\x3A-\x40\x5B-\x60\x7B-\x7E]).+$/;

export function AuthForm({
  mode,
  busy,
  error,
  onSubmit,
  onSwitch,
}: {
  mode: 'login' | 'signup';
  busy: boolean;
  error: string;
  onSubmit: (mode: 'login' | 'signup', input: AuthInput) => Promise<void>;
  onSwitch: () => void;
}) {
  const signup = mode === 'signup';
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [visible, setVisible] = useState(false);
  const [validation, setValidation] = useState('');
  const confirmationRef = useRef<HTMLInputElement>(null);
  return (
    <section
      className={`auth-card${signup ? ' auth-card-signup' : ''}`}
      aria-labelledby="auth-heading"
    >
      <header className="auth-brand">
        <img src="/ace-logo.png" alt="ACE" />
      </header>
      <h1 id="auth-heading">{signup ? 'ACE 시작하기' : 'ACE에 로그인'}</h1>
      <p className="auth-description">
        {signup
          ? '계정을 만들고 ACE와 함께 시작하세요.'
          : '다시 만나 반가워요. ACE가 기다리고 있어요.'}
      </p>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (busy) return;
          if (signup && !passwordComposition.test(password)) {
            setValidation('비밀번호에 영문, 숫자, 기호를 각각 하나 이상 포함해 주세요.');
            return;
          }
          if (signup && password !== confirmation) {
            setValidation('비밀번호가 일치하지 않습니다.');
            confirmationRef.current?.focus();
            return;
          }
          setValidation('');
          void onSubmit(mode, { email, password, displayName });
        }}
        aria-busy={busy}
      >
        <fieldset disabled={busy}>
          {signup && (
            <div className="auth-field">
              <label htmlFor="display-name">
                표시 이름 <span>선택</span>
              </label>
              <input
                id="display-name"
                className="text-field"
                autoComplete="nickname"
                maxLength={80}
                placeholder="어떻게 불러드릴까요?"
                value={displayName}
                onChange={(event) => setDisplayName(event.target.value)}
              />
            </div>
          )}
          <div className="auth-field">
            <label htmlFor="email">이메일</label>
            <input
              id="email"
              className="text-field"
              type="email"
              autoComplete="username"
              autoFocus
              required
              maxLength={254}
              placeholder="you@example.com"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
            />
          </div>
          <div className="auth-field">
            <label htmlFor="password">비밀번호</label>
            <div className="auth-password">
              <input
                id="password"
                className="text-field"
                type={visible ? 'text' : 'password'}
                autoComplete={signup ? 'new-password' : 'current-password'}
                required
                minLength={8}
                maxLength={128}
                aria-describedby={signup ? 'password-hint' : undefined}
                value={password}
                onChange={(event) => {
                  setPassword(event.target.value);
                  setValidation('');
                }}
              />
              <button
                type="button"
                aria-label={visible ? '비밀번호 숨기기' : '비밀번호 표시'}
                aria-pressed={visible}
                onClick={() => setVisible(!visible)}
              >
                {visible ? <EyeOff size={17} /> : <Eye size={17} />}
              </button>
            </div>
            {signup && (
              <p className="auth-hint" id="password-hint">
                8~128자, 영문·숫자·기호를 각각 하나 이상 포함해 주세요.
              </p>
            )}
          </div>
          {signup && (
            <div className="auth-field">
              <label htmlFor="password-confirmation">비밀번호 확인</label>
              <input
                ref={confirmationRef}
                id="password-confirmation"
                className="text-field"
                type={visible ? 'text' : 'password'}
                autoComplete="new-password"
                required
                maxLength={128}
                aria-invalid={!!validation}
                aria-describedby={validation ? 'auth-error' : undefined}
                value={confirmation}
                onChange={(event) => {
                  setConfirmation(event.target.value);
                  setValidation('');
                }}
              />
            </div>
          )}
          {(validation || error) && (
            <p id="auth-error" className="auth-error" role="alert">
              {validation || error}
            </p>
          )}
          <button type="submit" className="primary auth-submit" disabled={busy}>
            {busy ? (
              <>
                <LoaderCircle size={16} className="spin" />
                {signup ? '계정 만드는 중…' : '로그인 중…'}
              </>
            ) : (
              <>
                {signup ? '회원가입' : '로그인'}
                <ArrowRight size={16} />
              </>
            )}
          </button>
        </fieldset>
      </form>
      <p className="auth-switch">
        {signup ? '이미 계정이 있나요?' : '아직 계정이 없나요?'}{' '}
        <button type="button" disabled={busy} onClick={onSwitch}>
          {signup ? '로그인으로 돌아가기' : '계정 만들기'}
        </button>
      </p>
    </section>
  );
}
