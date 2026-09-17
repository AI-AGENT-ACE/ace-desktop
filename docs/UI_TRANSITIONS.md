# ACE 창 전환 및 트레이 동작

최대화·복원과 최소화는 `src/lib/windowAdapter.ts`에서 Tauri의 `toggleMaximize()`와 `minimize()`를 즉시 호출합니다. 버튼과 타이틀바 더블클릭 모두 적용됩니다.

이전 최대화 효과는 캡처 이미지를 늘려 표시했기 때문에 텍스트까지 늘어났다가 최종 화면에서 선명해졌습니다. 캡처 이미지 확대·축소 및 작업표시줄로 빨려 들어가는 효과를 제거했습니다. 실제 창을 바로 변경하고 WebView가 최종 크기에 맞춰 레이아웃을 계산합니다. Windows 자체의 창 효과는 운영체제가 관리합니다.

Rust 스냅샷 Overlay 모듈, Windows UI Automation/GDI 직접 의존성, 창 애니메이션 시간 변수도 제거했습니다.

X 버튼은 기존과 동일하게 창을 숨기고 프로세스와 대화·설정 상태를 유지합니다. `hide_ace` IPC는 기존 숨김 동작을 그대로 수행합니다. Alt+F4도 `CloseRequested`에서 숨깁니다. 트레이의 **ACE 열기** 또는 좌클릭으로 복원하고 **ACE 종료**로 완전히 종료합니다.

다크·라이트 테마의 300ms 색상 전환은 유지합니다. `prefers-reduced-motion`에서는 색상 전환을 비활성화합니다.

검증: `npm run build`, `npm run test:e2e -- tests/titlebar.spec.ts`, `cargo build --offline`. 타이틀바 테스트는 기본 창 API 호출, 애니메이션 IPC 미호출, X 버튼의 숨김 명령을 확인합니다.

Rust 변경 반영에는 앱 재시작이 필요합니다. 트레이 **ACE 종료**로 종료 후 다시 실행하세요. 개발 중에는 터미널 Ctrl+C 후 `npm run tauri dev`를 실행합니다.
