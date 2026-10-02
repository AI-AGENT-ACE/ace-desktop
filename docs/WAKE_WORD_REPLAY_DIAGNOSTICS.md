# Wake Word 실패 입력 녹음 및 재현

## 현재 확인한 사실

- 사용자의 기존 로그에서는 오디오 Worker가 계속 동작했고 드롭은 0이었다.
- active 모델 해시와 실제 저장 모델 해시가 일치하며 reference는 5개다.
- 기존 등록 완료 경로는 staging 디렉터리를 삭제한다. 현재 사용자 저장소에는
  settings.json과 user-reference.rpw만 있고, 등록 WAV와 테스트 WAV는 없다.
- 따라서 기존 등록 음성의 앞뒤 무음이나 실제 발화 길이를 측정했다고 말할 수 없다.
- 이번 변경은 재현/계측 도구다. 사용자의 실제 미감지 원인은 아직 미확정이며
  threshold 0.52 / avg_threshold 0.22 / min_scores 2 / eager false를 유지한다.

## 1. 실제 실패 입력 녹음

Windows 개발 빌드에서 실행한다.

```powershell
cd D:\ACE\ace-desktop
npm run tauri dev
```

1. 로그인 후 설정을 열고 Wake Word가 **듣는 중**인지 확인한다.
2. **개발용 Wake Word 진단 녹음**을 펼친다.
3. 종류를 **ACE 5회**로 선택하고 **진단 녹음 시작**을 누른다.
4. 상태가 `recording`이 되면 **침묵 3초 → 평소 목소리로 ACE 5회
   (각 2~3초 간격) → 침묵 3초**를 수행한다.
5. **종료 및 저장**을 누르고 `saved`와 저장 폴더를 확인한다.
6. 종류를 **침묵**으로 바꾸고 약 15초간 조용히 녹음한다.
7. 종류를 **ACE 없는 일반 발화**로 바꾸고 약 20초간 평소 대화를 녹음한다.

30초가 되면 자동으로 저장한다. 음성 명령용 VAD/무음 자동 종료를 추가한 것이
아니라 이 개발용 수집에만 적용한 상한이다. 설정 창을 닫아도 30초 상한은 유지된다.

시작 시 listener를 재시작해 **새 엔진의 첫 처리 프레임부터** 수집한다.
재현 시에도 같은 초기 상태를 사용한다. 수집 중에는 최종 감지를 계속 집계하되
Orb 활성화/마이크 pause만 보류한다. 종료 후에는 기존 Orb 동작으로 돌아간다.
이 자료는 KWS 검증용이며 Orb 이벤트/UI 실행 성공의 증거는 아니다.

CPAL callback은 변경하지 않았다. Worker에서 mono 변환 후 Rustpotter의
`process_samples()`에 실제 전달되는 완전한 프레임만 복사한다.
F32 입력은 F32 WAV, I16 입력은 I16 WAV로 저장하며 sample rate도 그대로다.
예: 현재 48000Hz/2ch/F32 장치는 Worker에서 downmix되므로 저장 WAV는
**48000Hz/1ch/F32**이다. 이는 Rustpotter 내부 16kHz 리샘플링 전 입력이다.

파일 I/O는 Worker에서 종료 시 수행하며 callback에서는 하지 않는다.
녹음은 명시적 시작 때만 메모리에 수집하고 PCM/모델은 서버로 전송하지 않는다.

## 2. 저장 자료

Windows 기본 위치:

```text
%LOCALAPPDATA%\com.ace.desktop\wake-diagnostics\<녹음 UUID>\
  input.wav            실제 엔진 입력 PCM
  model.rpw            그 listener가 실제 로드한 모델 바이트 사본
  manifest.json        PCM/모델 SHA-256, 전체 설정, 종류, 품질, 종료 사유
  live-trace.json      모든 처리 프레임의 실제 계측 결과
```

설정 화면에도 실제 저장 폴더가 표시된다. 음성은 사용자가 해당 폴더를 직접
삭제할 때까지 남는다. 소스 저장소 안에 저장하거나 Git에 올리지 않는다.
새로 등록해 active 모델이 바뀌어도 이 사본으로 녹음 당시 모델을 재현한다.
초기화 실패/리스너 종료 자료는 manifest의 reason과 프레임 수를 먼저 확인한다.

## 3. 재현 실행

별도 PowerShell에서 가장 최근 녹음을 재현한다.

```powershell
cd D:\ACE\ace-desktop
$diagnosticRoot = Join-Path $env:LOCALAPPDATA 'com.ace.desktop\wake-diagnostics'
$capture = Get-ChildItem -LiteralPath $diagnosticRoot -Directory |
  Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'manifest.json') } |
  Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $capture) { throw '저장된 진단 녹음이 없습니다.' }
cargo run --offline --manifest-path src-tauri/Cargo.toml --example replay_wake_capture -- $capture.FullName
```

특정 녹음은 마지막 인자를 설정 화면에 표시된 녹음 폴더의 전체 경로로 바꾼다.
녹음/재현 중 소스 설정을 바꾸지 않는다. 설정 문자열이나 PCM/모델 해시가 다르면
도구가 중단한다. 실시간과 재현은 `wake_kws::config`, `wake_kws::feed`를 공유하며
청크 사이에서 pending 프레임과 엔진 상태를 유지한다.

파일 끝에는 모델 최대 특징 길이에 기반한 충분한 후행 무음을 추가한다.
`live_prefix_equal=true`는 후행 무음 **이전의 모든 프레임 기록**이 일치한다는 뜻이다.
불일치하면 비정상 종료하며, 원인을 조사하기 전에 threshold 실험에 사용하면 안 된다.
WAV는 사용자 발화 시점으로 편집하거나 앞뒤 무음을 잘라서 재현하지 않는다.

결과 파일: `replay-baseline.json`. `detection_ms`는 입력 시작 기준 감지 확정 시간이며
후행 무음에서 확정된 감지도 포함된다. 후보 횟수는 MFCC 비교 통과 횟수이고 발화 횟수가 아니다.

## 4. 계측 해석

Rustpotter 3.0.2를 저장소 내부 `src-tauri/vendor/rustpotter`에 두고 Cargo patch를
사용했다. 전역 registry는 수정하지 않았고 Apache-2.0 LICENSE를 보존했다.

| 값/단계 | 실제 의미 |
|---|---|
| buffering | 특징 창이 충분히 채워지지 않아 점수 계산 전 |
| vad_skipped | 엔진 VAD 때문에 계산 건너뜀. 현재 설정 VAD=None |
| average_below_threshold_reference_not_computed | 평균 점수 계산 후 기준 미달. 개별 reference 점수는 계산 안 됨 |
| reference_score_below_threshold | reference 점수를 계산했지만 score가 threshold 이하 |
| candidate_matches | 실제 comparator가 통과시킨 후보 수 |
| partial_active_frames | 모든 PCM 처리 프레임 중 partial이 남아 있던 프레임 수 |
| waiting_countdown | 후보가 있으나 감지 확정까지 대기 중 |
| min_scores_rejected | 확정 시점에 누적 후보 수가 min_scores 미달 |
| final_detections | 실제 process_samples 반환 감지 횟수 |
| max_average / max_score | 실제 계산된 점수의 최댓값. 미계산은 null |
| reference_maxima | 동일 모델 안에서 reference 순번별 관측 최고 점수 |

후보/평균/reference 점수는 모든 프레임을 계측한다. 원래 계산하지 않은 값을
진단을 위해 임의로 계산하거나 0점으로 가정하지 않는다. 점수는 PCM 1프레임당
여러 MFCC 비교가 나올 수 있으므로 counter와 frames는 같은 단위가 아니다.
상세 정보는 로컬 JSON에 저장하고 per-frame 터미널 로그는 출력하지 않는다.
release에서는 Rust 계측 모듈/명령이 컴파일에서 제외되고 UI도 production에 나오지 않는다.

## 5. 한 조건 비교

기본 재현이 일치한 뒤, 같은 파일에 gain normalization만 끈 실험을 할 수 있다.

```powershell
cargo run --offline --manifest-path src-tauri/Cargo.toml --example replay_wake_capture -- $capture.FullName --gain-off
```

결과는 `replay-gain-off.json`에 저장한다. 앱 설정이나 active 모델은 바뀌지 않는다.
실험에서는 `live_prefix_equal=false`가 나올 수 있다. baseline의 true와 구분한다.
positive·silence·negative 세 녹음 모두에 baseline과 동일 실험을 적용한다.
감지 시점/횟수, 미달 단계, 점수 분포를 함께 비교한다. ACE 녹음에서 5회보다 많이
감지되거나 침묵/일반 발화에서 감지되면 개선이라고 단정하지 않는다.
수동 발화 시점과 finalization 지연도 고려해야 하며 1개 녹음으로 일반 감지율을 주장하지 않는다.

## 6. 다음 등록에서 원본 품질 조사

추가 단일 조건 실험: `--reference-threshold=0.44`로 모델 내부 reference 기준만
바꿀 수 있다. `--gain-off`와 동시에 사용할 수 없다. 결과는
`replay-threshold-0.44.json`, 후보 모델은 `model-threshold-0.44.rpw`로 별도 저장하며
active 모델을 자동으로 덮어쓰지 않는다. 구체적 근거와 결과는
로컬에 보존한 비공개 실제 입력 분석 보고서를 참고한다.

먼저 현재 모델의 실패 녹음을 남긴 뒤, 필요할 때만 목소리를 다시 등록한다.
등록 화면의 **개발 진단용 원본 WAV 로컬 보관**을 켜면 등록 및 테스트 WAV가
`wake-diagnostics\registration`에 UUID 이름과 품질 JSON으로 보관된다.
거절된 샘플도 조사할 수 있도록 보관하며 기본값은 꺼짐이다.

품질 JSON에는 전체 길이, RMS, peak, clipping 표본 수,
앞뒤 quiet 구간과 active span 추정치가 있다.
구간은 10ms RMS >= 0.01 기준의 **진단용 진폭 추정**이며 VAD나 음성인식 판정이 아니다.
활성 구간이 없으면 구간 정보가 null이며 무음 전체를 발화라고 판정하지 않는다.
sample_name, kind, captured_at_ms로 등록 샘플과 테스트 WAV를 구분한다.

WebView 개발 콘솔의 `[ACE Wake Diagnostic] WebView audio settings`에서
getUserMedia 요청값과 실제 getSettings 반환값, AudioContext rate를 확인한다.
장치 ID와 사용자 경로는 출력하지 않는다.

| 경로 | 설정/변환 |
|---|---|
| 등록 WebView | mono 요청, echoCancellation/noiseSuppression/autoGainControl=false 요청. 실제 지원값은 getSettings로 확인 |
| 등록 저장 | AudioContext rate의 float 입력을 채널 평균 후 mono I16 WAV로 저장 |
| 상시 WASAPI | CPAL 기본 입력 장치/default config. WebView의 장치 선택이나 DSP 설정과 동일하다고 가정하지 않음 |
| 상시 Worker | 실제 channels를 평균하여 mono, I16 또는 F32 타입 유지 |
| 엔진 | 입력 rate로 설정 후 Rustpotter 내부 변환. 앱에서 별도 리샘플링하지 않음 |

브라우저 요청값만으로 Windows 드라이버의 DSP까지 비활성이라고 판단하지 않는다.

## 검증 범위

공개 브랜치에서 다시 실행한 명령과 결과는 PR 설명에 기록합니다.
개인 음성 fixture에 의존하는 테스트는 파일을 공개하지 않으므로 제외합니다.
나머지 자동 테스트 결과도 실제 사용자 마이크 검증을 의미하지 않습니다.
사용자가 위 세 녹음을 저장한 이후, 해당 파일로 실제 미감지 원인을 분석하고
근거가 확인된 변경만 적용한다. 이번 단계에서 미감지가 해결됐다고 선언하지 않는다.

## 공개 PR의 음성 데이터 제외

개인정보 및 출처가 확인되지 않은 음성 공개를 피하기 위해 WAV와 사용자 모델은 포함하지 않습니다. 해당 파일을 사용하는 테스트는 명시적으로 ignore 처리합니다. 실제 사용자 보정 모델과 개별 발화 분석 보고서는 로컬 백업에 보존합니다.
