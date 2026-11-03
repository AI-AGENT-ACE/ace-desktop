# 개발 앱 updater 초기화 오류 수정

- 기본 tauri.conf.json에는 plugins.updater가 없고, 개발 실행 스크립트는 CSP만 병합함. Release CI에서만 prepare-update-release.mjs가 updater 설정을 생성함.
- 개발 빌드에서도 updater를 무조건 등록하여 누락된 설정(null)을 Config로 역직렬화하다 앱 시작이 실패함.
- debug_assertions 빌드에서는 플러그인 등록을 제외하고 updater IPC도 거부함. React 개발 모드에서는 상태 polling과 확인/설치 호출을 하지 않고 사용 불가 안내를 표시함.
- Release에서는 공개키가 빌드 공개키와 일치하고 지정한 HTTPS 업데이트 주소가 존재하는지 확인한 뒤 플러그인을 등록함. 기존 CI 공개키/개인키 준비 절차와 다운로드 서명 검증을 유지함.
- 기본 Wake Word 모델의 Release 검증은 변경하지 않음. 실제 공개 모델과 서명 키가 준비되지 않은 상태에서 Release 성공으로 처리하지 않음.

## 검증 (2026-10-16)

- npm run build 성공. 운영 산출물에서 Native 개발 패널 제외 검사 통과.
- Playwright 개발 모드 updater IPC 미호출 테스트 1개 통과.
- npm run tauri dev로 실제 Windows WebView2 앱 시작 확인. 기존 로그인 세션으로 메인 화면과 Native 개발 패널 표시 확인.
- 패널의 http://127.0.0.1:3001 표시 확인. 설정 화면에서 개발 모드 안내와 비활성 업데이트 버튼 확인.
- 실제 Rust check_app_update IPC가 UPDATE_UNAVAILABLE_IN_DEVELOPMENT를 반환하는 것 확인.
- 실제 Release 업데이트 설치/서명 실패 시나리오는 이번에 실행하지 않음. 음성 인식 정확도와 Native 작업 실행 시험도 이번 검증에 포함하지 않음.
