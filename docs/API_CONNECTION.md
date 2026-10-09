# React ↔ NestJS Baseline 연결

## Render 운영 빌드 (2026-10-07)

공개 운영 주소는 `https://ace-backend-cd8i.onrender.com`이며 전역 `/api` prefix는 없습니다. `GET /health/live`는 `200 {"status":"ok"}`를 반환합니다.

| 항목                          | 설정                                                               |
| ----------------------------- | ------------------------------------------------------------------ |
| 환경변수                      | 기존 `VITE_API_BASE_URL` 유지                                      |
| 개발                          | 기존 `.env`, `.env.local`과 `.env.example`의 로컬 백엔드 설정 유지 |
| 운영                          | Git에 포함할 `.env.production`에 Render 주소 지정                  |
| 웹 빌드                       | `npm run build`                                                    |
| Windows 실행 파일 검증 빌드   | `npm run tauri build -- --no-bundle`                               |
| Windows 설치 패키지 로컬 생성 | `npm run tauri build` (게시하지 않음)                              |

Vite의 production 모드에서는 `.env.production`이 공통 `.env`와 `.env.local`보다 우선합니다. 다만 프로세스 환경변수와 `.env.production.local`은 이를 덮어쓸 수 있으므로 다른 주소로 설정하지 마세요. API 주소는 빌드할 때 포함되며 설치 후 OS 환경변수를 바꾸어도 바뀌지 않습니다. `VITE_*`는 공개 클라이언트 값입니다. DB URL, JWT 서명 비밀키, AI 서버 비밀키를 넣지 않습니다.

메인 창의 Axios와 Orb(`VoiceWindow.tsx`)는 `src/api/client.ts`의 동일한 주소를 사용합니다. Orb는 그 주소를 Rust `upload_voice_recording`에 전달하고, Rust reqwest는 `/voice/commands`로 요청합니다. HTTP 플러그인(`@tauri-apps/plugin-http`, `tauri-plugin-http`)은 사용하지 않으므로 `http:default` 등의 capability를 추가하지 않았습니다. Rust 요청에는 브라우저 CORS가 적용되지 않으며 기존 TLS 인증서 검증을 유지합니다.

`scripts/tauri.mjs`는 선택된 API Origin만 CSP `connect-src`에 넣습니다. 기본 `tauri.conf.json`에도 정확한 Render Origin을 추가했으며 기존 개발용 loopback 허용은 유지합니다. CSP 전체 해제, 와일드카드 네트워크 권한, 인증 우회는 없습니다.

### Render CORS 설정과 실제 Windows Origin 확인

현재 main과 voice 창 모두 기본 Tauri App 프로토콜을 사용하고 `useHttpsScheme`을 켜지 않았으므로 Windows 운영 Origin은 `http://tauri.localhost`입니다. Orb의 `#voice` fragment는 Origin에 영향을 주지 않습니다. Render `CORS_ORIGINS`의 기존 필요한 값에 **`http://tauri.localhost`**를 쉼표로 구분해 추가하세요. 끝에 `/` 또는 `/api`를 붙이지 않습니다. Render 서버 주소 자체는 데스크톱 Origin이 아닙니다.

- Windows 설치 앱만 허용할 때: `CORS_ORIGINS=http://tauri.localhost`
- 개발 웹앱에서도 Render를 호출해야 할 때만 `http://localhost:1420`, `http://127.0.0.1:1420` 등 실제 개발 Origin을 추가합니다.
- 향후 `useHttpsScheme: true`로 변경하면 `https://tauri.localhost`도 필요합니다. macOS/Linux 지원 시에는 실제 `tauri://localhost` Origin을 확인합니다.

설치한 앱을 실행해 DevTools를 열 수 있으면 Console에서 `location.origin`을 확인하고 Network에서 API 요청의 Request Headers → `Origin`을 확인합니다. main과 Orb를 각각 확인하세요. 토큰이 담긴 Authorization 헤더나 저장소 내용은 공유하지 않습니다.

운영 앱에서 DevTools 단축키가 비활성화된 경우에는 앱을 트레이까지 완전히 종료하고, 아래처럼 현재 PowerShell 세션에서만 WebView2 원격 디버깅을 켜서 실제 설치 실행 파일을 실행할 수 있습니다. 경로는 설치된 파일로 바꿉니다.

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9222'
Start-Process -FilePath 'C:\실제\설치경로\ace-desktop.exe' -WindowStyle Hidden
Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
```

Edge의 `edge://inspect`에서 `localhost:9222`를 대상에 추가하고 ACE의 main/voice WebView를 Inspect하여 `location.origin`을 확인합니다. 확인 후 디버깅으로 실행한 ACE를 완전히 종료하고 평소 방식으로 재실행합니다. 이 설정을 설치 패키지나 시스템 환경변수에 영구 추가하지 않습니다.

2026-10-07 확인 시 운영 liveness는 정상(200)이지만 `Origin: http://tauri.localhost` 요청에 `Access-Control-Allow-Origin`이 없었습니다. 서버가 살아 있어도 브라우저 요청은 CORS로 차단되므로 Render 환경변수 반영 후 다시 확인해야 합니다. Free 서비스의 콜드 스타트는 기존 Axios 10초 timeout을 넘을 수 있습니다. 기존 인증/저장 요청 재시도 정책은 변경하지 않았습니다.

참고: [Tauri Windows 프로토콜 설정](https://v2.tauri.app/reference/config/#windowconfig), [Tauri HTTP 플러그인](https://v2.tauri.app/plugin/http-client/), [WebView2 디버깅](https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/debug-visual-studio-code).

## HTTP와 인증

`VITE_API_BASE_URL`의 서버에 `/api` Prefix 없이 요청합니다. Axios 인스턴스는 `src/api/client.ts` 하나입니다. 인증 API를 제외한 요청은 Access Token Bearer 인증이며 Refresh Token은 재발급 Body에만 전송합니다. 토큰은 현재 WebView의 sessionStorage에 저장하고 로그에 출력하지 않습니다. 앱을 완전히 종료하면 다시 로그인합니다.

401은 공통 Interceptor에서 한 번 재시도합니다. 병렬 401은 Refresh 하나를 공유하며 회전 전 Access Token의 늦은 401은 추가 Refresh 없이 새 Token으로 요청합니다. Refresh 실패 시 세션을 제거합니다. 계정 변경 후 늦은 Refresh 결과로 이전 세션을 복구하지 않습니다.

페이지 응답은 실제 계약인 `{ items, nextCursor, hasMore }`입니다. 대화는 `isPinned`, 메시지 Role은 `USER/ASSISTANT/TOOL/SYSTEM`입니다. 설정 수정 요청은 응답의 부가 필드를 제외하고 허용된 필드만 전송합니다.

## React 상태와 페이지

일반 목록과 휴지통은 서로 다른 Hook·API·상태입니다. 각각 20개씩 조회하고 IntersectionObserver와 더 보기 버튼을 제공합니다. 요청 중인 Controller를 Ref에 저장해 같은 커서의 동시 요청을 막고 항목 ID로 중복을 제거합니다. 삭제·복구·영구 삭제 성공 후 현재 상태에서 제거하며 복구 후 일반 목록은 수동 재조회합니다. 이동/삭제된 항목이 현재 커서 경계일 때만 첫 페이지를 재조회해 서버의 목록 소유권 조건에 맞는 커서를 복구합니다.

메시지는 최신순으로 받아 각 페이지를 뒤집어 오래된 순으로 표시합니다. 이전 30개는 앞에 누적합니다. 이전 scrollHeight와 scrollTop을 저장한 뒤 추가된 높이만큼 보정합니다. 성공 응답의 메시지는 현재 상태에 추가하며 이전 페이지를 유지합니다.

GET은 AbortController로 취소하고 요청 세대를 비교해 이전 대화의 늦은 응답을 무시합니다. 최초 로딩·추가 페이지·전송·오류를 구분합니다. 실패한 요청을 Runtime Mock 데이터로 대체하지 않습니다. 저장 요청은 무조건 자동 재전송하지 않습니다.

## Agent

사용자 말풍선은 즉시 표시하며 `/conversations/:id/messages`로 먼저 저장합니다. AI 설정 시 저장된 `messageId`와 본문을 `/agent/turns`에 보내므로 사용자 메시지가 중복 저장되지 않습니다. 중지 버튼은 AI 요청을 취소하고 저장된 사용자 메시지를 남깁니다. 새 데스크톱보다 `messageId`를 지원하는 백엔드를 먼저 적용해야 합니다. 기존 클라이언트처럼 `messageId`를 생략하는 요청도 백엔드가 지원합니다.

첫 메시지를 임시 제목으로 저장하고 사이드바의 긴 제목은 말줄임표로 표시합니다. 추후 Modal `/agent/text` 응답 최상위에 선택적 `title`이 오면 임시 제목만 교체합니다. 사용자가 직접 바꾼 제목은 보존합니다. 답변의 타이핑 표시는 전체 응답 수신 후 화면에서 수행하며 토큰 스트리밍 API는 아닙니다.

도구 티켓은 `/agent/cloud-tools` 또는 `/agent/tool-results`로 전달합니다. Cloud 작업은 서버, Local 작업은 Tauri Rust에서 실행합니다. 원본 Tool 결과는 일시 전달하며 별도로 저장하지 않습니다.

## IPC와 음성

앱 실행과 허용 폴더의 파일 열기는 기본 허용이며 사용자의 ASK/DENY 설정을 우선합니다. 앱 종료·파일 변경/삭제 등은 기존 확인 정책을 유지합니다. 승인 UI는 메인 창의 모달 대신 항상 위에 표시되는 Orb의 선택 버튼으로 제공합니다. Shell 실행 및 허용 폴더 밖 접근 제한은 유지합니다. 날씨 조회는 도구 목록과 기능에서 제외했습니다.

음성 창은 `voice` 라벨의 별도 WebviewWindow로, 이 창에만 always_on_top을 적용합니다. cloud 모드는 인증된 NestJS `/voice/commands`를 거쳐 Modal `/agent/voice`를 사용합니다. Orb의 승인 응답은 로컬 STT로 ‘네/아니오/번호’를 해석하며 클릭도 가능합니다. 녹음이 비었거나 너무 짧으면 짧은 재시도 문구를 표시하고, 재청취 중 5초 침묵이면 닫습니다. 일반 호출어는 기본 모델로 사용하고 개인 5회 녹음은 선택 사항입니다.

`POST /logs/voice`는 commandType/status/duration/errorCode만 받습니다. 원문 필드는 DTO가 거부하고 Repository도 명시적 필드 투영을 사용합니다. 로컬 로그는 클라이언트 보고이며 서버의 OS 실행 증명은 아닙니다.

## 환경과 테스트

성능 비교를 위해 개발 모드에서만 `window.aceApiMetrics.snapshot()`과 `clear()`를 제공합니다. 최근 500개 요청의 method·익명화 route·status·durationMs·outcome만 기록하며 Token, Body, Query, 음성 원문, 실제 대화 ID는 수집하지 않습니다. 요청 시작 전에 취소될 수도 있어 cancelled 항목은 별도로 비교하고 실제 Network Request/Transferred Data는 DevTools Network로 확인합니다. `performance.now` 기반 클라이언트 시간은 Backend 처리 시간·DB Query 횟수를 대신하지 않습니다. 목록 재진입·페이지 추가·메시지 전송 시 같은 데이터와 조건에서 snapshot을 저장해 비교할 수 있습니다. 요청 정보만 기록하며 서버 응답 캐시는 없습니다.

페이지 Hook은 items/nextCursor/hasMore/isLoading/isLoadingMore/error 6개 State와 현재 커서·Controller·요청 세대 3개 Ref를 직접 관리합니다. 메시지는 같은 Hook을 사용해 이전 페이지를 앞에 누적합니다. 코드량은 `src/hooks/useCursorList.ts`, `useMessages.ts`, `useInfiniteScroll.ts`를 기준으로 비교하세요.

CORS는 Vite/Tauri Origin을 명시적으로 허용합니다. Bearer 인증이므로 credentials=false입니다. `npm run tauri dev/build` 래퍼는 환경변수 API Origin을 CSP에 반영합니다. 개발 모드에서는 Vite Refresh용 inline script와 명시적인 WebSocket Origin을 허용합니다.

통합 테스트의 `ace-backend/scripts/browser-test-server.mjs`는 운영 진입점과 분리되어 로컬 `*_test` DB만 허용합니다. `/__test/fixture`는 테스트 진입점에만 존재합니다. 데이터와 테스트 AI를 실제 앱에 사용하지 않습니다. 검증 대상은 데이터 재조회·20/30 페이지 누적·스크롤·중복 전송·설정 저장·단일 Refresh·음성 최소 로그·테마·타이틀바입니다. 외부 AI/날씨 서버와 실제 Windows 앱 환경은 이 테스트의 검증 범위에 포함하지 않습니다.
