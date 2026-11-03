import { useEffect, useRef, useState } from 'react';
import { authApi } from '../../api/auth.api';
import { apiErrorMessage } from '../../api/client';

export function ProfileSettings() {
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const lock = useRef(false);
  const mounted = useRef(false);
  useEffect(() => {
    mounted.current = true;
    const controller = new AbortController();
    void authApi
      .me(controller.signal)
      .then((user) => {
        if (controller.signal.aborted) return;
        setName(user.displayName || '');
        setEmail(user.email);
        setLoaded(true);
      })
      .catch((cause) => {
        if (!controller.signal.aborted) setError(apiErrorMessage(cause));
      });
    return () => {
      mounted.current = false;
      controller.abort();
    };
  }, []);
  return (
    <form
      className="profile-editor"
      onSubmit={async (event) => {
        event.preventDefault();
        if (lock.current || !loaded) return;
        const value = name.trim();
        if (!value || [...value].length > 80) {
          setError('이름은 1~80자로 입력해 주세요.');
          return;
        }
        lock.current = true;
        setBusy(true);
        setError('');
        setNotice('');
        try {
          const user = await authApi.updateProfile(value);
          if (mounted.current) {
            setName(user.displayName || '');
            setNotice('프로필을 저장했습니다.');
          }
        } catch (cause) {
          if (mounted.current) setError(apiErrorMessage(cause));
        } finally {
          lock.current = false;
          if (mounted.current) setBusy(false);
        }
      }}
    >
      <h3>프로필</h3>
      <p>{email}</p>
      <label>
        표시 이름{' '}
        <input
          aria-label="표시 이름"
          value={name}
          disabled={!loaded || busy}
          onChange={(event) => {
            setName(event.target.value);
            setNotice('');
          }}
        />
      </label>
      <button type="submit" disabled={!loaded || busy}>
        {busy ? '저장 중…' : '프로필 저장'}
      </button>
      {error && <p role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}
    </form>
  );
}
