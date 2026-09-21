# ACE 트레이 제어 허브

## 메뉴

트레이는 다음 순서로 구성한다.

1. **ACE 열기**: 기존 `main` 창을 표시하고 최소화 상태를 복원한 뒤 포커스를 준다. 새 창을 만들지 않는다.
2. **음성 호출**: React의 음성 버튼과 같은 Rust `activate_voice_orb` 흐름을 실행한다.
3. **ACE 숨기기**: `main` 창만 숨기며 프로세스와 트레이·Orb 런타임은 유지한다.
4. **Wake Word: 켜짐/꺼짐**: Rust 런타임 상태와 메뉴 문구를 함께 변경하고 main UI에 이벤트를 전달한다.
5. **설정**: main 창을 열고 React 설정 모달을 표시한다. 인증 화면에 있을 때 발생한 요청은 보류했다가 메인 화면 진입 시 소비한다.
6. **종료**: 음성 세션 상태를 초기화하고 Orb를 숨긴 뒤 프로세스를 종료한다.

왼쪽 트레이 아이콘 클릭은 **ACE 열기**와 동일하다. 타이틀바 X와 Alt+F4는 계속 main 창만 숨기며 실제 종료는 트레이 메뉴에서 수행한다.

## 음성 진입점

`src-tauri/src/voice_overlay.rs`의 `activate()`가 내부 단일 진입점이다. 트레이 메뉴와 `activate_voice_orb` IPC가 모두 이 함수를 호출하므로 별도 Orb나 별도 세션을 만들지 않는다.

- 앱 시작 시 `voice` 창 하나를 만들고 숨겨 둔다.
- 활성화 시 현재 모니터 오른쪽 아래에 32px 간격으로 배치한다.
- `always_on_top`을 다시 확인하고 표시·포커스한다.
- 이미 활성 상태이면 입력 초기화 이벤트를 다시 보내지 않고 기존 창을 표시한다.
- 첫 활성화는 WebView에 마이크 시작 이벤트를 보내고 `getUserMedia({ audio: true })`로 오디오 스트림을 요청한다.
- 명령 제출·닫기·앱 종료 시 모든 마이크 트랙을 중지하고 활성 상태와 임시 입력을 초기화한다.
- 창 표시가 실패하면 활성 상태를 되돌리고 트레이 작업 이름과 함께 오류를 남긴다.

향후 Wake Word 감지기와 Global Shortcut도 Rust `activate()`를 호출하면 같은 중복 방지 정책을 사용할 수 있다.

## Wake Word의 현재 범위

현재 Orb는 마이크 스트림을 시작하지만 STT·Wake Word 감지 엔진은 연결되어 있지 않다. 따라서 트레이 토글은 **런타임 활성화 선호 상태**를 변경하고 React 설정과 동기화하지만 실제 백그라운드 Wake Word Listener를 시작했다고 표시하지 않는다. 설정 화면과 Orb도 감지 미연결 상태를 명시한다.

Wake Word가 꺼져 있어도 수동 트레이 음성 호출은 동작한다. 실제 감지기를 연결할 때는 `TrayState`의 상태를 시작·중지 조건으로 사용하고 감지 결과가 `voice_overlay::activate()`만 호출하도록 연결해야 한다.

Orb는 LISTENING 화면과 마이크 스트림을 제공하지만 음성을 문자로 변환하지 않는다. 명령은 현재 수동으로 입력한다. 마이크 권한 거부와 장치 오류는 Orb에 표시하며 이미 획득한 트랙은 즉시 정리한다.

## 구조와 보안

- `src-tauri/src/tray.rs`: 메뉴 ID, `TrayAction`, 런타임 상태, 메뉴 생성과 이벤트 처리
- `src-tauri/src/voice_overlay.rs`: Orb 단일 창·세션 상태·위치·표시·정리
- `src-tauri/src/lib.rs`: 상태 등록, 모듈 조합, 창 닫기 정책
- `src/App.tsx`: 설정/Wake Word 이벤트 수신과 로컬 UI 동기화

트레이 이벤트는 Rust 내부에서 실행되므로 새 WebView capability나 wildcard 권한을 추가하지 않았다. 기존 main capability의 최소 창 권한만 유지한다.

## 검증

```powershell
cd src-tauri
cargo fmt -- --check
cargo check --offline
cargo test --offline

cd ..
npm run build
npm run test:e2e
```

Rust 테스트는 메뉴 ID의 중앙 매핑, Wake Word 메뉴 문구, 기존 로컬 명령 정책과 작업표시줄 아이콘을 검사한다. 브라우저 테스트는 설정 상태와 기존 창 제어가 깨지지 않는지 검사한다. 실제 트레이 메뉴와 네이티브 창 배치는 `npm run tauri dev`에서 확인한다.
