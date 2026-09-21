import { useRef, useState } from 'react';
import { Check, CircleAlert, Eye, EyeOff, LoaderCircle, X } from 'lucide-react';
import type { AuthInput, AuthResult } from '../hooks/useAuth';

const passwordRules = [
  { label: '8자 이상', test: (value: string) => value.length >= 8 },
  { label: '영문 포함', test: (value: string) => /[A-Za-z]/.test(value) },
  { label: '숫자 포함', test: (value: string) => /[0-9]/.test(value) },
  {
    label: '기호 포함',
    test: (value: string) => /[\x21-\x2F\x3A-\x40\x5B-\x60\x7B-\x7E]/.test(value),
  },
] as const;

export function AuthForm({
  mode,
  initialEmail,
  success,
  busy,
  error,
  onSubmit,
  onSwitch,
}: {
  mode: 'login' | 'signup';
  initialEmail: string;
  success: string;
  busy: boolean;
  error: string;
  onSubmit: (mode: 'login' | 'signup', input: AuthInput) => Promise<AuthResult>;
  onSwitch: () => void;
}) {
  const signup = mode === 'signup';
  const [email, setEmail] = useState(initialEmail);
  const [password, setPassword] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [visible, setVisible] = useState(false);
  const [rememberMe, setRememberMe] = useState(false);
  const [showPasswordRules, setShowPasswordRules] = useState(false);
  const [confirmationTouched, setConfirmationTouched] = useState(false);
  const [validation, setValidation] = useState('');
  const confirmationRef = useRef<HTMLInputElement>(null);
  const nicknameRef = useRef<HTMLInputElement>(null);
  const passwordValid = passwordRules.every((rule) => rule.test(password));
  const showConfirmationState = signup && confirmationTouched && confirmation.length > 0;
  const confirmationMatches = showConfirmationState && password === confirmation;
  return (
    <section
      className={`auth-card${signup ? ' auth-card-signup' : ''}`}
      aria-labelledby="auth-heading"
    >
      <header className="auth-brand">
        <img src="/ace-logo.png" alt="ACE" />
      </header>
      <h1 id="auth-heading">{signup ? 'ACE에 회원가입' : 'ACE에 로그인'}</h1>
      <p className="auth-description">{signup ? '계정을 생성하여 계속하세요.' : '계정에 로그인하여 계속하세요.'}</p>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (busy) return;
          if (signup && !displayName.trim()) {
            setValidation('닉네임을 입력해 주세요.');
            nicknameRef.current?.focus();
            return;
          }
          if (signup && !passwordValid) {
            setValidation('비밀번호 조건을 모두 충족해 주세요.');
            setShowPasswordRules(true);
            return;
          }
          if (signup && password !== confirmation) {
            setValidation('비밀번호가 일치하지 않습니다.');
            confirmationRef.current?.focus();
            return;
          }
          setValidation('');
          void onSubmit(mode, { email, password, displayName, rememberMe });
        }}
        aria-busy={busy}
      >
        <fieldset disabled={busy}>
          {signup && (
            <div className="auth-field">
              <label htmlFor="display-name">닉네임</label>
              <input
                ref={nicknameRef}
                id="display-name"
                className="text-field"
                autoComplete="nickname"
                required
                minLength={1}
                maxLength={80}
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
              placeholder="ace@gmail.com"
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
                onFocus={() => signup && setShowPasswordRules(true)}
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
            {signup && showPasswordRules && (
              <ul className="auth-password-rules" id="password-hint" aria-label="비밀번호 조건">
                {passwordRules.map((rule) => {
                  const met = rule.test(password);
                  return (
                    <li key={rule.label} className={met ? 'met' : 'unmet'}>
                      {met ? (
                        <Check size={13} aria-hidden="true" />
                      ) : (
                        <X size={13} aria-hidden="true" />
                      )}
                      {rule.label}
                    </li>
                  );
                })}
              </ul>
            )}
          </div>
          {signup && (
            <div className="auth-field">
              <label htmlFor="password-confirmation">비밀번호 확인</label>
              <input
                ref={confirmationRef}
                id="password-confirmation"
                type={visible ? 'text' : 'password'}
                autoComplete="new-password"
                required
                maxLength={128}
                className={`text-field${showConfirmationState ? (confirmationMatches ? ' confirmation-match' : ' confirmation-mismatch') : ''}`}
                aria-invalid={showConfirmationState ? !confirmationMatches : undefined}
                aria-describedby={
                  showConfirmationState
                    ? 'password-confirmation-status'
                    : validation
                      ? 'auth-error'
                      : undefined
                }
                value={confirmation}
                onFocus={() => setConfirmationTouched(true)}
                onChange={(event) => {
                  setConfirmation(event.target.value);
                  setConfirmationTouched(true);
                  setValidation('');
                }}
              />
              {showConfirmationState && (
                <p
                  id="password-confirmation-status"
                  className={`auth-confirmation-status ${confirmationMatches ? 'match' : 'mismatch'}`}
                  aria-live="polite"
                >
                  {confirmationMatches ? (
                    <Check size={13} aria-hidden="true" />
                  ) : (
                    <X size={13} aria-hidden="true" />
                  )}
                  {confirmationMatches ? '비밀번호가 일치합니다.' : '비밀번호가 일치하지 않습니다.'}
                </p>
              )}
            </div>
          )}
          {(validation || error) && (
            <p id="auth-error" className="auth-error" role="alert">
              <CircleAlert size={15} aria-hidden="true" />
              {validation || error}
            </p>
          )}
          {!signup && success && (
            <p className="auth-success" role="status">
              <Check size={15} aria-hidden="true" />
              {success}
            </p>
          )}
          {!signup && (
            <label className="auth-remember">
              <input
                type="checkbox"
                checked={rememberMe}
                onChange={(event) => setRememberMe(event.target.checked)}
              />
              <span>자동 로그인</span>
            </label>
          )}
          <button type="submit" className="primary auth-submit" disabled={busy}>
            {busy ? (
              <>
                <LoaderCircle size={16} className="spin" />
                {signup ? '계정 만드는 중…' : '로그인 중…'}
              </>
            ) : signup ? (
              '회원가입'
            ) : (
              '로그인'
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
