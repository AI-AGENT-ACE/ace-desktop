# ACE Desktop

React + TypeScript + Vite + Tauri 2 앱입니다. Axios로 [ACE NestJS 백엔드](https://github.com/AI-AGENT-ACE/ace-backend)에 연결합니다. 서버 상태는 기본 React 상태와 Custom Hook으로 관리하며 TanStack Query·Redis를 사용하지 않습니다.

## 실행

Node.js 22.12 이상, Tauri 실행에는 Rust·Windows C++ 빌드 도구·WebView2가 필요합니다.

```powershell
git clone https://github.com/AI-AGENT-ACE/ace-desktop.git
cd ace-desktop
npm ci
Copy-Item .env.example .env
```

`.env`의 `VITE_API_BASE_URL`을 실행 중인 백엔드 주소로 설정합니다. 기본 예시는 `http://127.0.0.1:3001`입니다. 프론트 환경변수에는 Secret이나 API Key를 넣지 않습니다. 별도 터미널에서 백엔드와 PostgreSQL을 먼저 실행하세요.

```powershell
npm run dev
# 데스크톱 앱:
npm run tauri dev
```

브라우저 주소는 http://localhost:1420 입니다. 회원가입 또는 로그인 후 사용합니다. 운영 주소는 `.env.production` 또는 빌드 환경의 `VITE_API_BASE_URL`로 지정합니다. `npm run tauri build`는 API Origin을 CSP에 반영합니다. 환경변수 변경 후 재시작/재빌드하고 운영 백엔드의 CORS에도 Tauri Origin을 등록하세요.

## 구현

- 회원가입·로그인·로그아웃·프로필 조회·Access Token 재발급
- 대화 생성·조회·이름 변경·고정·휴지통 이동·복구·영구 삭제
- 대화 20개씩 무한 스크롤, 메시지 최근 30개·이전 30개 누적
- 이전 메시지 추가 시 스크롤 위치 유지, Markdown, 중복 전송 방지
- 응답 언어·TTS 선호·도구 권한의 계정 설정 저장
- AI 설정 시 Agent 대화·Cloud/Local Tool 결과 전달
- AI 미설정 시 실제 메시지 API를 통한 저장 모드
- 별도 always-on-top 음성 입력 창, 원문을 제외한 최소 실행 로그
- Rust 정책 검증, 시스템 상태 조회, 승인 후 메모장·계산기 실행/종료 요청
- 커스텀 타이틀바, ACE 아이콘, 트레이, 테마 전환

타이틀바를 드래그해 이동하고 더블클릭으로 최대화/복원합니다. 최대화/복원·최소화는 기본 창 API를 즉시 호출합니다. X/Alt+F4는 창을 숨기며 트레이의 ‘ACE 열기’ 또는 좌클릭으로 다시 열고 ‘ACE 종료’로 완전히 종료합니다.

## 외부 서비스와 범위

AI 응답은 백엔드의 `AI_SERVER_URL`과 실제 AI 서버가, 날씨는 `WEATHER_API_KEY`가 필요합니다. 설정 여부 표시는 실시간 가용성 검증이 아닙니다. 실패한 AI 요청은 자동 재전송하지 않으며 저장된 사용자 메시지를 재조회합니다.

STT·마이크·Wake Word 감지·TTS 출력은 미연결입니다. 음성 창에 ‘시스템 상태 조회’, ‘메모장 실행/종료’, ‘계산기 실행/종료’를 직접 입력해 로컬 실행 흐름을 사용할 수 있습니다. 임의 앱·파일·Shell 명령은 차단하며 앱 종료에 강제 종료 옵션을 사용하지 않습니다. 브라우저 미리보기에서는 로컬 명령을 실행하지 않습니다.

음성 원문은 대화나 localStorage에 저장하지 않습니다. 테마·Wake Word 선호는 이 PC에, 계정 설정은 서버에 저장됩니다. 휴지통 30일 만료 정리는 백엔드 책임입니다.

## 구조와 검증

- `src/api/`: 단일 Axios Client와 기능별 API
- `src/hooks/`: React 커서 조회, 중복 요청 방지, 요청 취소
- `src/features/`: 인증·대화·설정·휴지통·음성·로컬 명령
- `src-tauri/src/`: 트레이·타이틀바·IPC 정책·음성 창

```powershell
npm run build
npm run test:e2e
cd src-tauri
cargo test --lib
```

브라우저 통합 테스트에는 형제 폴더의 `ace-backend`, 백엔드 빌드(`npm run build`), 실행 중인 로컬 PostgreSQL, 별도 `*_test` DB가 필요합니다. 백엔드 `.env`의 `TEST_DATABASE_URL`을 사용합니다. 테스트 서버 3002와 프론트 1430을 별도로 실행하며 인증·대화·메시지·설정·로그는 실제 PostgreSQL, AI는 명시적인 테스트 어댑터를 사용합니다. Windows Edge를 사용하며 다른 환경에서는 `PLAYWRIGHT_BROWSER_PATH`를 지정하세요.

[API 연결 기록](docs/API_CONNECTION.md), [UI 전환 기록](docs/UI_TRANSITIONS.md), [아이콘 기록](docs/APP_ICONS.md)에 변경 내용을 정리했습니다. `.env`, 의존성 폴더, 빌드 결과물, 로컬 DB, 테스트 산출물, 개인 인증서는 Git에서 제외합니다. `.env.example`과 lock 파일은 설치 재현을 위해 포함합니다.
