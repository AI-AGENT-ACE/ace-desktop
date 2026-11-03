# clipboard.read 직접 실행 검증 — 2026-10-22

이 문서는 당시의 로컬 전용 전달 계약에 대한 과거 검증이다. 2026-10-26 채팅 원문 전달 및 읽기/쓰기 기본 권한 변경은 [후속 기록](CLIPBOARD_CHAT_RESULT_2026-10-26.md)을 따른다. 아래 실제 Windows 통과를 새 계약 전체의 재검증으로 간주하지 않는다.

검증 커밋 미기록(로컬 변경). 사용자가 합의한 테스트 문장을 직접 복사하고 '복사 완료'라고 응답한 뒤에만 실제 읽기를 수행함. 원문 결과 UI는 기존 구현을 재사용하고, 누락된 개발 패널 항목과 다음 요청에서 이전 결과 제거만 추가. 음성·볼륨 실제 시험 없음.

## 상태 구분

- clipboard.read: 과거 채팅은 Native 실행·결과 로그 없이 이전 대화 내용을 답했으므로 AI 도구 호출 누락 의심. Rust 읽기 실패로 확정하지 않음. 아래 직접 실행 통과가 채팅 문제 해결을 뜻하지 않음.
- voice.activate: 실제 동작 대기. /voice/commands의 VOICE_PROCESSING_FAILED는 녹음 후 음성 처리 단계이며 활성화·녹음 시작 실패 근거로 사용하지 않음.
- system.volume.mute: AI가 '볼륨 꺼줘'를 app.close와 '볼륨' 대상으로 반환한 의도 오류. Native 음소거 실패 아님. NestJS의 엄격한 mute:boolean 계약과 Rust의 bool 검사에 system.volume.mute/{"mute":true}가 일치함. 실제 음소거는 대기.

## 실제 Windows·로컬 DB 결과

실제 Tauri 개발 앱 → 로컬 NestJS 권한/티켓 → Rust 준비·일회성 승인·실행 경로. 기존 테스트 계정임을 이메일 접두사/도메인으로 확인. API/IPC 응답 모의 교체 없음. 클립보드 변경 API를 사용하지 않음.

| 시나리오 | 결과 |
|---|---|
| DENY | 준비/실행 호출 없음, 결과 UI 없음 |
| ASK 취소 | Rust prepare만 호출, authorize/execute 없음, CANCELLED |
| 새 요청 승인 | prepare → authorize → execute, UI 문장이 사용자 테스트 문장과 정확히 일치, SUCCEEDED |
| 읽기 후 보존 | 읽기 전후 Windows ClipboardSequenceNumber 동일. 승인 후 OS 읽기로 기대 문장 일치 여부만 비교하여 true 확인(원문 출력 없음) |
| 성공 후 DENY | 이전 결과 UI 제거, 새 읽기 없음 |
| 다시 승인 후 ASK 취소 | 성공 결과를 표시한 뒤 다음 요청에서 제거, 취소 시 읽기 실행 없음 |

실제 DB NativeExecution 4행: CANCELLED 2건, SUCCEEDED 2건. DENY는 사전 권한 차단으로 실행 등록 없이 종료. 실행 ID:
- 취소 403a0605-1abd-4efa-8f04-8dd426f6a7f9
- 성공 4bdd6ff5-dc02-42d5-b87d-d3fb14ccc032
- 성공 342017ff-c11f-41e5-bf5c-a8ec2731998a
- 취소 859901a3-38ff-414b-ac79-3007b20a4d07

해당 NativeExecution/NativeExecutionEvent 행 및 일치하는 Rust 감사 로그 2건에서 원문 미포함 확인. 이번 요청 HTTP body에도 원문 미전송. 전체 과거 DB·운영 로그를 검사했다는 뜻은 아님. 증거 `.local/clipboard-verification.json`에는 ID·상태·일치 여부만 저장.

## 수정·자동 검증

- Native 테스트 패널에 clipboard.read 추가. 빈 인자, 기존 권한/승인 경로 유지.
- queueLocalRequest와 공통 execute 시작 시 클립보드 결과 초기화. 사전 DENY 및 승인 취소에 이전 내용이 남지 않도록 처리.
- TypeScript/프런트 빌드와 운영 산출물 개발 패널 제외 검사 통과. 기존 큰 청크 경고 유지. 이번 실제 Windows 결과를 모의 테스트로 대체하지 않음.

## AI 담당자 전달 사항

- 클립보드: 재현 '클립보드에 복사한 텍스트 읽어줘'. 제공 로그에 실행/결과 전송이 없고 이전 문장 답변. 도구 호출 누락 의심이며 원본 AI JSON 미확보. 기대 clipboard.read, arguments={}. 도구 결과 수신 후 성공/거절/실패에 맞춰 안내. 현재 원문은 로컬 UI 전용이고 AI에는 상태 요약만 전달하므로 원문을 추측·복원해 답하지 않음. AI가 원문을 분석/낭독하는 기능은 별도 동의·전송 계약이 필요한 범위로 이번에 추가하지 않음.
- 음소거: 재현 '볼륨 꺼줘'. 이전 실제 AI 조사에서 app.close와 대상 '볼륨' 반환, 사용자에게 앱 미설치 안내. 기대 system.volume.mute, arguments={"mute":true}. 실제 실행 결과 수신 후에만 완료 안내. Native 실행 성공 여부는 아직 대기.

AI 코드·운영 환경 변경, 음성·볼륨 시험, 커밋·푸시·배포 없음. 추가 사용자 조작 불필요. 설치 앱 및 실제 AI 채팅 흐름의 최신 검증은 별도 대기.
