# 인증 UI 및 진입 흐름

검증: `npm run build`, `npm run test:e2e`(실제 NestJS/PostgreSQL 연동과 인증 UI), `npm run test:auth`(DB 없이 인증 UI 오류·상태 전환 검사). 인증 전 요청 차단, 복원 중 Splash, 비밀번호 확인, 가입 DTO, 중복 제출·Enter, 비밀번호 표시, 한국어 오류, 갱신 실패를 브라우저에서 검사한다.

## 적용 범위

`AuthGate`는 인증 상태에 따라 Splash → 로그인/회원가입 또는 기존 `AceApp`을 렌더링한다. 라우터가 없는 기존 구조를 유지하며 인증 폼 내부 state로 화면을 전환한다. 메인 사이드바·채팅·헤더·설정 디자인과 Tauri 창 제어는 유지한다.

- `features/auth/hooks/useAuth.ts`: checking/authenticated/unauthenticated 상태, 로그인·가입·로그아웃, 세션 변경 감지.
- `features/auth/components/AuthLayout.tsx`: 같은 메인 창의 인증 레이아웃과 저장된 테마 적용.
- `features/auth/components/AuthForm.tsx`: 로그인·가입 폼, 비밀번호 확인과 표시/숨김, 접근성, 중복 제출 제한.
- `styles/api.css`: 기존 테마 변수에 따른 인증 화면 스타일.

## 실제 API 및 검증

기존 공통 Axios를 사용한다. `POST /auth/login`은 email/password, `POST /auth/register`는 email/password와 선택 displayName을 받는다. 비밀번호는 8~128자이며, 신규 가입은 영문(A–Z/a–z), 숫자, ASCII 기호를 각각 하나 이상 포함해야 한다. 공백이나 한글은 기호 조건을 충족하지 않는다. 기존 계정의 로그인은 조합 조건을 강제하지 않는다. 프론트와 백엔드 DTO가 모두 검증한다. 이메일 최대 254자, 이름 최대 80자로 제한한다. 비밀번호 확인은 프론트에서 검사하며 서버로 보내지 않는다. 가입은 토큰과 user를 반환하므로 즉시 메인 화면으로 진입한다.

인증 화면의 기본 Edge/WebView2 비밀번호 표시 버튼은 `::-ms-reveal`을 숨겨 커스텀 버튼과의 중복을 방지한다. 기본 버튼은 사용자가 입력하면 나타나고 포커스를 잃으면 사라지는 브라우저 동작이므로 React 버튼을 중복 생성한 문제가 아니다. 입력 타입을 전환하는 기존 커스텀 버튼은 유지한다. 카드 테두리와 브랜드 슬로건을 제거하고 하단 표기를 `ACE Auto Computer Executor`로 변경했다.

초기 저장된 토큰이 있으면 `GET /users/me`로 확인한다. 완료 전에는 메인 화면 및 메인 데이터 API를 시작하지 않는다. 401은 기존 단일 동시 갱신 정책을 사용하며 갱신 실패 시 세션을 제거하고 로그인 화면으로 전환한다. 복원 요청은 unmount/세션 제거 시 취소하여 뒤늦은 응답이 메인 화면을 다시 열지 못하도록 한다.

로그아웃은 기존 설정 버튼에서 `POST /auth/logout`을 호출하고 실패하더라도 로컬 세션을 제거한다. `AceApp` unmount로 대화·메시지·설정 등의 사용자 상태를 제거한다. 서버는 전역 AccessTokenGuard와 userId 소유권 검사를 이미 적용하고 있으며 이를 유지한다.

## 저장 정책과 한계

기존 `sessionStorage` 토큰 정책을 유지한다. 새로고침과 같은 WebView 세션에서는 복원할 수 있고, X 버튼으로 트레이에 숨겼다가 다시 열어도 유지된다. 프로세스를 완전히 종료하고 다시 실행하면 다시 로그인한다. 완전 종료 후에도 로그인을 유지하려면 별도 보안 저장 정책을 합의해야 하며 이번 UI 작업에서 임의로 변경하지 않았다.

비밀번호는 폼 state에만 있고 전환/성공 시 폼 unmount로 제거된다. 토큰 저장 시 user는 제외하고 토큰 쌍만 저장한다. 요청 본문·토큰·헤더를 로그로 출력하지 않는다. 인증 오류는 HTTP 상태를 한국어로 매핑하여 서버의 raw message/JSON/stack을 출력하지 않는다. 새 상태 라이브러리, TanStack Query, Redis, OAuth, 비밀번호 재설정 기능은 추가하지 않았다.
