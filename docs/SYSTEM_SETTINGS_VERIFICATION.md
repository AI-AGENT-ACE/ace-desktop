# Windows 설정 열기 검증 — 2026-10-22

검증 커밋: 미기록(로컬 작업 상태). 기존 8개 정상 실행 및 실제 덮어쓰기→DB 검증은 재실행 없이 유지.

## 실제 Windows 개발 앱 결과

AI 없는 Native 패널 → 실제 로컬 NestJS 권한·티켓 → Rust system.open_settings → 공통 ShellExecuteW 경로. native-matrix/native-settings 접두사의 example.test 테스트 계정임을 확인한 뒤에만 권한 변경. 개인 계정 권한 및 Windows 설정값 변경 없음.

| 조건 | 관찰 | 판정 |
|---|---|---|
| DENY | 실제 API로 차단 설정. Rust 실행 호출 0, 실행 이벤트 0, 기존 설정 창/페이지 변화 없음 | 통과 |
| ASK 취소 | REQUESTED → CANCELLED, Rust 실행 호출 0, 창/페이지 변화 없음 | 통과 |
| 새 요청 승인·display | 별도 attemptId로 요청·승인, Rust 실행, RUNNING → SUCCEEDED, 실제 디스플레이 해상도·배율 UI 표시 | 통과 |
| 기본 설정(page="") | 마지막 별도 시험에서 요청 전 창 없음 → HWND 919526 설정 창 생성 및 홈 UI 표시 | 통과 |
| 기존 설정 창 | 기본 URI가 기존 페이지를 유지할 수 있음. 별도 승인 시 foreground=false → true 관찰 | 존재만으로 성공 판정하지 않음 |

증거: `.local/settings-verification.json`, `.local/settings-approve-home.json`, `.local/settings-deny.json`. 취소 attemptId c89e7531-560d-4e1e-8099-6a1400d804d6, display e2282a37-dfee-4d39-9239-5efc8b0511b6, 기본 738cf788-a860-415b-9863-3f8e5df88989. 이번에는 실제 HTTP 이벤트를 관찰했으며 DB 직접 조회로 표현하지 않음.

실제 WebView2 UI를 CDP로 조작. 실제 invoke 진입을 debugger로 관찰하고 재개했으며 모의 응답/실행 우회 없음. Windows UI Automation으로 고정된 화면 이름만 조회. 초기 관찰은 SystemSettings만 찾아 ApplicationFrameHost 창을 놓쳤으므로 관찰기 보완 후 확인. 시험 창 닫기 시도는 이미 핸들이 사라져 대상 검사에서 중단했으며 다른 창을 닫지 않음. 이후 요청 전 창 없음과 기본 홈 새 표시를 확인.

## 변경 및 자동 검증

- 개발 패널에 기본/디스플레이 설정 선택 추가. 자유 URI/명령 입력 없음. 기존 권한·티켓 경로 유지.
- Rust의 page=null/숫자를 기본 화면으로 처리하던 동작을 INVALID_ARGUMENT 거부로 수정. 기존 허용 목록 유지.
- 새 Native 요청에서 이전 실행 상태 초기화. DENY 시 이전 완료 문구가 남던 문제 수정 후 실제 차단 재시험에서 제거 확인.
- NestJS 계약 테스트 4개(설정 1개 포함), Rust 설정 허용 목록 테스트 1개 통과. 기본/display 허용, 임의 URI·명령·잘못된 자료형 거부. NestJS 추가 필드 거부 포함. 이는 단위 시험이며 실제 악성 요청 OS E2E 시험이 아님.
- 실제 개발 앱 재빌드·실행, TypeScript·프런트 빌드·운영 산출물 Native 패널 제외 검사 통과. 기존 큰 청크 경고 유지.

## 대기

모든 세부 설정 페이지·다른 Windows 버전·설정 앱 고장/OS 접근 거부·AI 채팅 의도 변환은 대기. 이번 기본/display·DENY·ASK 취소/새 승인에는 추가 사용자 조작 불필요. 음성·클립보드·볼륨·잠금 검증, 운영 변경, 커밋·푸시·배포 없음.
