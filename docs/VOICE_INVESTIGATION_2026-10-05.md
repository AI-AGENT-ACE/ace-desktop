# 2026-10-05 음성 명령 문제 분석 및 수정

## 원인과 변경

1. **발화 후 녹음이 끝나지 않음**: VoiceWindow는 PCM을 계속 append하며 Orb 클릭만 stopAndSave를 호출했다. 무음 자동 종료는 구현되어 있지 않았다. 에너지 기반 종료를 추가했다. RMS 0.01 이상 입력 누적 250ms 뒤 1.5초 무음이면 기존 writeQueue → WAV finalize → upload 경로를 사용한다. 발화 없는 입력은 10초, 발화가 있는 입력은 30초로 제한한다. 문장 중 짧은 쉼과 초기 무음은 즉시 종료하지 않는다. 에너지 판정이므로 소음 환경·조용한 목소리에 대해서 실제 마이크 검증이 필요하다.
2. **메인에서 로그인했는데 Orb는 로그인 요구**: voice Webview는 앱 시작 시 미리 생성된다. 각 창의 session 모듈은 시작 시 storage를 한 번 읽고 메모리의 pair를 반환한다. 메인의 이후 로그인은 voice pair를 갱신하지 않는다. 자동 로그인 미선택 시 sessionStorage도 창마다 분리된다. voice가 자신의 getSession을 읽는 코드를 제거하고, 업로드 직전 main으로 인증 요청을 보내도록 했다. main의 /users/me와 기존 Axios 공유 Refresh Promise로 세션을 검증/갱신하고, 요청 ID가 일치하는 voice 창에만 Access Token을 전달한다. Refresh Token은 main에 유지하며 voice storage에 복사하지 않는다. 로그아웃·세션 만료 및 서버 연결 실패를 구분한다.
3. **실제 음성 AI 서버 미설정**: 현재 backend .env의 AI_SERVER_URL이 비어 있으며 실행 중 /health/ready도 ai=not_configured였다. 인증을 고쳐도 /voice/commands에서 STT·명령 해석을 완료할 수 없다. Rust가 503의 공개 AI_SERVER_UNAVAILABLE 코드를 구분하고 Orb에서 음성 AI 서버 연결 필요를 안내한다. 다른 일시적 503은 기존 제한 재시도를 유지한다. 실제 AI URL/필요한 인증키가 있어야 명령을 수행할 수 있다.

## Wake Word 재현

새 PC의 positive 21초 / negative 30초 / silence 20.31초 진단 자료를 읽었다. 원본은 보존하고 Git에서 제외된 .local/voice-investigation/captures 사본에서 실험했다. 세 baseline 모두 모델·PCM checksum 검증 및 live_prefix_equal=true였다.

| 개인 모델 reference 기준 | ACE 녹음 감지 | 일반 발화 | 침묵 |
|---|---:|---:|---:|
| 0.52 기존 | 1 | 0 | 0 |
| 0.48 실험 | 4 | 0 | 0 |
| 0.46 실험 | 4 | 0 | 0 |

기존 ACE 입력 max_score=0.521337로 0.52를 간신히 넘고 reference gate 탈락이 다수였다. 기준 하나만 바꿔 감지가 늘어났으므로 reference gate가 미감지의 주요 제한임을 확인했다. 발음·거리·마이크 DSP 중 무엇이 낮은 유사도의 근본 원인인지는 확정하지 않았다. 일반 발화와 침묵에서의 짧은 실험 0회는 장기간 오탐률 보장이 아니다. 4회를 5/5 성공으로 표현하지 않는다.

전역 기준과 AppData active 모델은 변경하지 않았다. 0.48/0.46 후보 모델은 로컬 실험 사본 안에 생성되어 있다. 권장 후속 순서는 새 마이크에서 실제 명령과 같은 거리·음량으로 재등록 → 별도의 ACE 5회 녹음과 긴 일반 발화로 평가 → 필요할 때 개인 모델 기준 보정 → 실제 Orb 활성화 검증이다. 기준을 무조건 더 낮추는 것은 해결로 보장되지 않는다.

## 검증 및 실행

- TypeScript 검사 및 Vite production build 성공. 실행 도구의 Windows spawn EPERM으로 기본 Vite config bundling이 간헐적으로 실패하여 `vite build --configLoader native`도 확인했다.
- `node --experimental-vm-modules scripts/test-voice-regression.mjs`: 3 passed. 실제 session/client/voice-session 모듈을 분리된 JS context에 로드하고 Tauri event transport 및 HTTP adapter만 대체했다. 이미 생성된 voice 창, 이후 main 로그인, 만료 Access, 동시에 발생한 401의 Refresh 1회, Refresh Token 미복사, 로그아웃 거절, listener 정리 및 종료 조건을 검사했다. 실제 WebView/마이크 E2E와 구분한다.
- `cargo test --offline --manifest-path src-tauri/Cargo.toml voice_recording`: 6 passed.
- `cargo clippy --offline --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`: 성공.
- Playwright 회귀 spec과 전용 config를 추가했지만 실행 도구에서는 spawn EPERM으로 시작하지 못했다. 성공으로 기록하지 않는다.
- 기존 backend dist/main.js를 실행했다. DB connected, ai not_configured를 확인했다. DB migration/reset이나 계정 데이터 변경은 수행하지 않았다.
- Vite를 `npm run dev -- --configLoader native`로 실행하고 로컬 override(.local/voice-investigation/tauri-dev.json)의 빈 beforeDevCommand를 사용하여 Tauri를 시작했다. 개발 런타임 컴파일과 프로세스 시작을 확인했다. 이 도구에서 실행한 프로세스는 WASAPI stream build에서 접근 거부(os error -2147024891)가 나서 실제 사용자 발화 테스트는 완료하지 못했다. 사용자 터미널의 원래 실행에서 감지된 현상과 구분한다.

## 사용자가 확인할 흐름

ACE를 Tray에서 종료한 뒤 수정본을 사용자 터미널에서 다시 실행한다. 메인에서 로그인(자동 로그인 미선택 포함) → Tray 음성 호출로 먼저 Orb 확인 → 명령 발화 후 1.5초 무음 → 녹음 종료와 AI 미설정 안내 확인. 실제 AI 서버 설정 후 같은 경로로 STT와 도구 실행을 확인한다. 마지막으로 Wake Word 재등록·독립 평가를 진행한다. 원래 개인 모델과 진단 원본은 보존한다.

## 후속: 15회 이상 호출 무반응 로그와 개인 감도 설정

사용자가 제공한 대기 로그에서는 WASAPI stream build/play가 성공했고 14,008 callback, 4,669 engine frame까지 처리됐다. dropped=0, queue_depth=0~1이고 초기 Xrun 3회는 이후 증가하지 않았다. 발화 구간 RMS는 약 0.008~0.016, peak는 최대 0.248로 실제 입력이 들어왔다. final_detections=0이므로 이 실행은 Orb 전달 이전의 KWS에서 최종 감지가 없었다. partial_active=false는 10초마다 관찰한 순간의 상태이므로 모든 프레임에서 후보가 없었다고 확대하지 않는다. 현재 로그만으로 모든 발화의 gate 탈락 원인을 확정할 수는 없다.

동일 모델의 앞선 재현 결과에 근거해 **설정 → 호출 감도 → 민감하게**를 추가했다. 사용자가 선택하면 이 PC의 wake settings에 sensitivity를 저장하고 리스너를 재시작한다. runtime에서 reference threshold를 0.48로 적용한다. 기존 개인 모델 기준이 이미 더 낮으면 그 값을 유지한다. 기본 선택은 원본 모델 기준을 그대로 사용한다. 모델 파일 자체는 수정하지 않으므로 기본으로 복원 가능하고, 새 목소리 등록 완료 시 기본 감도로 시작한다. 녹음·등록·진단 진행 중 변경은 거부한다. 재시작 실패 시 이전 설정을 복원한다.

전역 config.threshold만 바꾸면 모델의 Some(threshold)가 우선하므로 효과가 없을 수 있다. 이번 변경은 실제 로드한 reference에 적용하며 `effective sensitivity` 로그로 적용값을 표시한다. 개발용 진단에는 이 유효 모델을 직렬화해 저장하므로 replay와 같은 모델·설정으로 비교할 수 있다.

일반 대기 중에도 개발 빌드의 Worker에서 비교 점수·gate 탈락을 집계하고 10초마다 `KWS gate diagnostics (last interval)`을 출력한다. 평균 gate 탈락으로 reference 점수가 계산되지 않았다면 max_score=None으로 표시한다. PCM이나 프레임별 추적을 상시 저장하지 않는다. 사용자가 켠 진단 녹음은 기존 전체 frame trace를 유지한다.

이번 변경은 설정 기능과 진단 보완이며, 사용자의 AppData 개인 설정을 도구에서 직접 변경하거나 새 마이크 감지 성공을 확인한 것은 아니다. 설정에서 ‘민감하게’ 적용 후 `reference_threshold=Some(0.48)`과 실제 Orb 활성화를 확인해야 한다. 기존 자료의 4회 개선을 새 발화 100% 성공으로 취급하지 않는다. 이번 후속 작업에서는 개발 서버나 앱 프로세스를 추가로 시작하지 않았다.
