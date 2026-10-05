import { useEffect, useState } from 'react';
import { Check, LoaderCircle, Mic, ShieldCheck } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { Modal } from '../../components/Modal';

type Phase = 'intro' | 'checking' | 'ready' | 'recording' | 'generating' | 'testing' | 'completed' | 'error';
type Quality = { accepted: boolean; message?: string; durationMs: number; rms: number; peak: number };
type Progress = { completedSamples: number; totalSamples: number };

export function WakeWordSetupModal({ onboarding = false, onClose, onCompleted }: {
  onboarding?: boolean;
  onClose: () => void;
  onCompleted: () => void;
}) {
  const [phase, setPhase] = useState<Phase>('intro');
  const [progress, setProgress] = useState<Progress>({ completedSamples: 0, totalSamples: 5 });
  const [error, setError] = useState('');
  const [isRecording, setIsRecording] = useState(false);
  const [retainDiagnostics, setRetainDiagnostics] = useState(false);
  const [testPasses, setTestPasses] = useState(0);
  const [busy, setBusy] = useState(false);
  const releaseMicrophone = async () => { setIsRecording(false); };

  useEffect(() => () => {
    void releaseMicrophone(); void invoke('cancel_wake_word_setup');
    if (import.meta.env.DEV) void invoke('retain_wake_setup_diagnostics', { enabled: false }).catch(() => undefined);
  }, []);

  const prepare = async () => {
    setPhase('checking');
    setTestPasses(0);
    setError('');
    try {
      const next = await invoke<Progress>('prepare_wake_word_setup');
      setProgress(next);
      setPhase('ready');
    } catch {
      setError('마이크를 사용할 수 없습니다. 마이크 연결과 Windows 권한을 확인해주세요.');
      setPhase('error');
    }
  };

  const beginRecording = async (test = false) => {
    setError('');
    setBusy(true);
    try {
      if (import.meta.env.DEV) await invoke('retain_wake_setup_diagnostics', { enabled: retainDiagnostics });
      await invoke(test ? 'start_wake_word_test_sample' : 'start_wake_word_sample');
      setIsRecording(true);
      setPhase(test ? 'testing' : 'recording');
    } catch {
      await releaseMicrophone();
      setError('녹음을 시작하지 못했습니다. 마이크 권한을 확인해주세요.');
      setPhase(test ? 'testing' : 'ready');
    } finally { setBusy(false); }
  };

  const finishRecording = async (test = false) => {
    await releaseMicrophone();
    setBusy(true);
    try {
      const quality = await invoke<Quality>('finish_wake_word_sample');
      if (!quality.accepted) {
        if (test) setTestPasses(0);
        setError(quality.message || '녹음 품질을 확인할 수 없습니다. 다시 말해주세요.');
        setPhase(test ? 'testing' : 'ready');
        return;
      }
      if (test) {
        const detected = await invoke<boolean>('test_wake_word_reference');
        if (!detected) {
          setTestPasses(0);
          setError('아직 “ACE”를 정확히 인식하지 못했습니다. 다시 테스트하거나 목소리를 다시 등록해주세요.');
          setPhase('testing');
          return;
        }
        const passes = testPasses + 1;
        setTestPasses(passes);
        if (passes < 2) { setPhase('testing'); return; }
        await invoke('complete_wake_word_setup');
        setPhase('completed');
        onCompleted();
        return;
      }
      const completedSamples = progress.completedSamples + 1;
      setProgress((value) => ({ ...value, completedSamples }));
      if (completedSamples === progress.totalSamples) {
        setPhase('generating');
        await invoke('generate_wake_word_reference');
        setPhase('testing');
      } else setPhase('ready');
    } catch {
      if (test) setTestPasses(0);
      setError('음성 샘플을 처리하지 못했습니다. 다시 시도해주세요.');
      setPhase(test ? 'testing' : 'ready');
    } finally { setBusy(false); }
  };

  const close = async () => {
    await releaseMicrophone();
    await invoke('cancel_wake_word_setup').catch(() => undefined);
    onClose();
  };

  const postpone = async () => {
    await releaseMicrophone();
    await invoke('postpone_wake_word_setup').catch(() => undefined);
    onClose();
  };

  return <Modal title={onboarding ? 'ACE 시작하기' : '음성 호출 설정'} onClose={() => void close()}>
    <div className="wake-setup">
      {import.meta.env.DEV && <label><input type="checkbox" checked={retainDiagnostics} disabled={isRecording}
        onChange={event => setRetainDiagnostics(event.target.checked)} />개발 진단용 WAV 로컬 보관 (앞뒤 무음 정리 후 저장, 직접 삭제 전까지 유지)</label>}
      {phase === 'intro' && <>
        <div className="wake-setup-icon"><Mic size={28} /></div>
        <h2>음성으로 ACE를 호출할 수 있습니다</h2>
        <p>“ACE”라고 말하면 앱을 빠르게 호출할 수 있습니다. 선택 기능이며 나중에 설정할 수도 있습니다.</p>
        <div className="wake-privacy"><ShieldCheck size={17} /><span>목소리 데이터는 이 기기에서만 처리되며 서버로 전송되지 않습니다.</span></div>
        <div className="modal-actions"><button onClick={() => void close()}>나중에 하기</button><button className="primary" onClick={() => void prepare()}>음성 호출 설정하기</button></div>
      </>}
      {phase === 'checking' && <div className="wake-setup-center"><LoaderCircle className="spin" /><strong>마이크를 확인하고 있습니다</strong></div>}
      {(phase === 'ready' || phase === 'recording') && <>
        <h2>내 목소리 등록</h2><p>“ACE”라고 자연스럽게 한 번 말해주세요.</p>
        <div className="wake-dots" aria-label={`${progress.completedSamples} / ${progress.totalSamples}`}>
          {Array.from({ length: progress.totalSamples }, (_, index) => <span key={index} className={index < progress.completedSamples ? 'done' : ''} />)}
        </div>
        <strong className="wake-count">{progress.completedSamples} / {progress.totalSamples}</strong>
        {error && <p className="danger-text" role="alert">{error}</p>}
        {phase === 'recording' ? <button disabled={busy} className="wake-record active" onClick={() => void finishRecording()}><Mic size={20} />녹음 끝내기</button> : <button disabled={busy} className="wake-record" onClick={() => void beginRecording()}><Mic size={20} />녹음 시작</button>}
      </>}
      {phase === 'generating' && <div className="wake-setup-center"><LoaderCircle className="spin" /><strong>내 목소리에 맞게 준비하고 있습니다</strong></div>}
      {phase === 'testing' && <>
        <h2>실제 호출을 확인해볼게요 ({testPasses}/2)</h2><p>테스트 시작 후 1초 기다렸다가 “ACE”라고 한 번 말하고, 2초 더 기다린 뒤 끝내주세요. 두 번 연속 인식되면 완료됩니다.</p>
        {error && <p className="danger-text" role="alert">{error}</p>}
        {isRecording ? <button disabled={busy} className="wake-record active" onClick={() => void finishRecording(true)}><Mic size={20} />테스트 끝내기</button> : <button disabled={busy} className="wake-record" onClick={() => void beginRecording(true)}><Mic size={20} />테스트 시작</button>}
        <div className="modal-actions"><button disabled={busy} onClick={() => void postpone()}>나중에 하기</button><button disabled={busy || isRecording} className="text-button" onClick={() => void prepare()}>목소리 다시 등록</button></div>
      </>}
      {phase === 'completed' && (
        <div className="wake-setup-center">
          <Check size={30} />
          <h2>등록 테스트를 통과했습니다</h2>
          <p>
            실제 마이크 입력에서 ACE 호출을 두 번 확인했습니다.
            <br />
            등록과 상시 감지에 같은 마이크 입력 방식을 사용합니다.
          </p>
          <button className="primary" onClick={onClose}>
            완료
          </button>
        </div>
      )}
      {phase === 'error' && <><p className="danger-text" role="alert">{error}</p><div className="modal-actions"><button onClick={() => void close()}>나중에 하기</button><button className="primary" onClick={() => void prepare()}>다시 시도</button></div></>}
    </div>
  </Modal>;
}
