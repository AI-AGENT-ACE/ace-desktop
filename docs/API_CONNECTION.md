# React ↔ NestJS Baseline 연결

## HTTP와 인증

`VITE_API_BASE_URL`의 서버에 `/api` Prefix 없이 요청합니다. Axios 인스턴스는 `src/api/client.ts` 하나입니다. 인증 API를 제외한 요청은 Access Token Bearer 인증이며 Refresh Token은 재발급 Body에만 전송합니다. 토큰은 현재 WebView의 sessionStorage에 저장하고 로그에 출력하지 않습니다. 앱을 완전히 종료하면 다시 로그인합니다.

401은 공통 Interceptor에서 한 번 재시도합니다. 병렬 401은 Refresh 하나를 공유하며 회전 전 Access Token의 늦은 401은 추가 Refresh 없이 새 Token으로 요청합니다. Refresh 실패 시 세션을 제거합니다. 계정 변경 후 늦은 Refresh 결과로 이전 세션을 복구하지 않습니다.

페이지 응답은 실제 계약인 `{ items, nextCursor, hasMore }`입니다. 대화는 `isPinned`, 메시지 Role은 `USER/ASSISTANT/TOOL/SYSTEM`입니다. 설정 수정 요청은 응답의 부가 필드를 제외하고 허용된 필드만 전송합니다.

## React 상태와 페이지

일반 목록과 휴지통은 서로 다른 Hook·API·상태입니다. 각각 20개씩 조회하고 IntersectionObserver와 더 보기 버튼을 제공합니다. 요청 중인 Controller를 Ref에 저장해 같은 커서의 동시 요청을 막고 항목 ID로 중복을 제거합니다. 삭제·복구·영구 삭제 성공 후 현재 상태에서 제거하며 복구 후 일반 목록은 수동 재조회합니다. 이동/삭제된 항목이 현재 커서 경계일 때만 첫 페이지를 재조회해 서버의 목록 소유권 조건에 맞는 커서를 복구합니다.

메시지는 최신순으로 받아 각 페이지를 뒤집어 오래된 순으로 표시합니다. 이전 30개는 앞에 누적합니다. 이전 scrollHeight와 scrollTop을 저장한 뒤 추가된 높이만큼 보정합니다. 성공 응답의 메시지는 현재 상태에 추가하며 이전 페이지를 유지합니다.

GET은 AbortController로 취소하고 요청 세대를 비교해 이전 대화의 늦은 응답을 무시합니다. 최초 로딩·추가 페이지·전송·오류를 구분합니다. 실패한 요청을 Runtime Mock 데이터로 대체하지 않습니다. 저장 요청은 무조건 자동 재전송하지 않습니다.

## Agent

AI 설정 시 `/agent/turns`가 사용자 메시지 저장과 AI 호출을 담당합니다. 같은 입력을 메시지 API에 중복 전송하지 않습니다. AI 미설정 시 `/conversations/:id/messages`로 입력만 저장합니다. AI 장애 후에는 최근 서버 메시지를 조회해 이미 저장된 입력을 반영합니다.

도구 티켓은 `/agent/cloud-tools` 또는 `/agent/tool-results`로 전달합니다. Cloud 작업은 서버, Local 작업은 Tauri Rust에서 실행합니다. 원본 Tool 결과는 일시 전달하며 별도로 저장하지 않습니다.

## IPC와 음성

중앙 Frontend 정책과 Rust 최종 정책은 시스템 상태 조회 SAFE, 메모장·계산기 실행/종료 CONFIRM, 임의 명령·파일 작업 BLOCKED입니다. BLOCKED는 승인으로 우회할 수 없습니다. Rust는 호출 Window·승인 여부·인자 구조·허용 앱을 재검사합니다. Shell 문자열과 임의 실행 파일은 받지 않습니다.

음성 창은 `voice` 라벨의 별도 WebviewWindow로, 이 창에만 always_on_top을 적용합니다. 전용 Rust 명령으로 main에 입력을 일시 전달하고 폐기합니다. STT 미연결을 UI에 표시하며 음성 원문을 Agent/Message API에 보내지 않습니다.

`POST /logs/voice`는 commandType/status/duration/errorCode만 받습니다. 원문 필드는 DTO가 거부하고 Repository도 명시적 필드 투영을 사용합니다. 로컬 로그는 클라이언트 보고이며 서버의 OS 실행 증명은 아닙니다.

## 환경과 테스트

성능 비교를 위해 개발 모드에서만 `window.aceApiMetrics.snapshot()`과 `clear()`를 제공합니다. 최근 500개 요청의 method·익명화 route·status·durationMs·outcome만 기록하며 Token, Body, Query, 음성 원문, 실제 대화 ID는 수집하지 않습니다. 요청 시작 전에 취소될 수도 있어 cancelled 항목은 별도로 비교하고 실제 Network Request/Transferred Data는 DevTools Network로 확인합니다. `performance.now` 기반 클라이언트 시간은 Backend 처리 시간·DB Query 횟수를 대신하지 않습니다. 목록 재진입·페이지 추가·메시지 전송 시 같은 데이터와 조건에서 snapshot을 저장해 비교할 수 있습니다. 요청 정보만 기록하며 서버 응답 캐시는 없습니다.

페이지 Hook은 items/nextCursor/hasMore/isLoading/isLoadingMore/error 6개 State와 현재 커서·Controller·요청 세대 3개 Ref를 직접 관리합니다. 메시지는 같은 Hook을 사용해 이전 페이지를 앞에 누적합니다. 코드량은 `src/hooks/useCursorList.ts`, `useMessages.ts`, `useInfiniteScroll.ts`를 기준으로 비교하세요.

CORS는 Vite/Tauri Origin을 명시적으로 허용합니다. Bearer 인증이므로 credentials=false입니다. `npm run tauri dev/build` 래퍼는 환경변수 API Origin을 CSP에 반영합니다. 개발 모드에서는 Vite Refresh용 inline script와 명시적인 WebSocket Origin을 허용합니다.

통합 테스트의 `ace-backend/scripts/browser-test-server.mjs`는 운영 진입점과 분리되어 로컬 `*_test` DB만 허용합니다. `/__test/fixture`는 테스트 진입점에만 존재합니다. 데이터와 테스트 AI를 실제 앱에 사용하지 않습니다. 검증 대상은 데이터 재조회·20/30 페이지 누적·스크롤·중복 전송·설정 저장·단일 Refresh·음성 최소 로그·테마·타이틀바입니다. 외부 AI/날씨 서버와 실제 Windows 앱 환경은 이 테스트의 검증 범위에 포함하지 않습니다.
