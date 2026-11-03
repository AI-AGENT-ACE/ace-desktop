# Windows 전체 캡처 대기 수정 (2026-10-16)

## 원인

- 기존 Native 로그의 executionId `af299f19-117d-45ce-81ae-184b1ff702b8`은 모니터 목록 반환(SELECTION_REQUIRED)까지만 기록됨.
- 실제 수정 전 개발 앱에서 모니터 선택 후 about:blank 선택창과 계속 대기 상태를 재현함.
- 동기 execute_native_tool 명령 안의 WebviewWindowBuilder가 Windows UI 스레드를 막음. 설치된 Tauri 2.11.5 소스 webview_window.rs에도 동기 명령에서 생성 시 Windows 교착 주의사항이 명시됨.
- 승인 대기나 생성된 이미지의 UI 누락이 아니라 선택창 생성 중 진행이 막힌 경로임. 원래 사용자 실행 전체의 중간 로그가 없으므로 기존 실행의 모든 단계를 소급 확정한 것은 아님.

## 변경

- 실제 execute_native_tool 공통 IPC를 async로 전환하고 State는 소유한 AppHandle에서 조회함. 권한·호출 창·승인 ID·실행 ID 검증은 유지함.
- 캡처 완료·취소 IPC도 async 실행. 선택창 제거 중 세션 Mutex를 잡지 않도록 변경.
- 개발 로그 capture-stages.jsonl에 executionId/attemptId/sessionId/시각/단계를 기록함. 이미지 내용·파일 경로·토큰은 기록하지 않음.
- 실패·취소 시 세션 결과로 오류를 전달하고 선택창을 정리함. 대기 시간 초과에도 취소 처리함.
- 취소 이유를 메인 창과 개발 패널에 표시함. 성공 시 기존 결과 영역의 이미지 열기/다른 이름으로 저장/닫기 제공.

## 실제 Windows 검증

- 테스트 패널 → 모니터 전체 → 캡처를 실제 WebView2에서 조작. API·IPC를 mock하거나 승인 우회하지 않음.
- PNG signature 확인, 2880×1800, 1,403,526바이트 생성 확인. 이미지 픽셀 내용은 출력하지 않음.
- executionId `db051a48-dd9d-437b-9d4f-38e0d9ac9056`, 실제 캡처 attemptId `9c5dcb36-19a7-4cde-9d54-40e855f5d607`: selector_creating → selector_ready → capture_started → image_generated → result_ready → result_delivered. 로컬 백엔드 조회에서 SUCCEEDED 확인.
- 모니터 목록 요청은 별도 attempt이며 실제 캡처 attempt의 previousAttemptId로 연결됨. 동일 실행 ID 유지.
- 취소 executionId `f0fd6852-2184-412a-8699-90f12b95bae9`, attemptId `c9ef8696-2535-44f4-abfe-dd95bbd58322`: Esc 취소 후 CANCELLED 확인. 선택창 0개, 취소 안내 표시, 테스트 요청 버튼 활성화 확인.
- 최종 수정 코드로 전체 캡처 성공과 결과 표시 재확인.

## 자동 검증과 미검증

- Rust 캡처 테스트 3개 통과(좌표/영역 거부/임시 이미지 정리). 별도 GUI 영역 테스트 1개는 ignored 상태.
- 브라우저 테스트 5개 통과: 성공 결과·오류·취소·모니터 연결 해제 안내·영역 선택/Esc. 이 테스트의 Native 응답은 mock이며 실제 캡처 검증과 별개임.
- 실제 다중 모니터 연결 해제, 실제 Windows 캡처 API 강제 오류, 이미지 기본 프로그램 열기와 사용자 저장 동작은 이번 실제 시험에 포함하지 않음.
- 운영 Render/Neon/Release 변경 없음.
