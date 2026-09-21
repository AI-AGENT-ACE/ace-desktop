# Native 앱 실행 보안 정책

## 실행 흐름

```text
자연어 또는 Tool appName
→ 프론트 위험도 확인
→ 사용자 승인
→ Rust 명령 종류·인자 재검증
→ Installed App Registry 별칭 일치
→ canonical 실행 경로·허용 위치 재검증
→ 실행 또는 종료
```

## Installed App Registry

- 앱 시작 시 Windows `App Paths`, System32 기본 앱, VS Code 알려진 설치 위치를 탐색합니다.
- 매 실행 요청마다 시스템 전체를 스캔하지 않습니다.
- `%APPDATA%/com.ace.desktop/installed-apps.json`은 시작 속도를 위한 cache입니다.
- cache에 경로를 직접 추가해도 실행 권한이 생기지 않습니다. 현재 시작 과정에서 신뢰 가능한 Windows 정보원으로 다시 발견된 경로만 Registry에 들어갑니다.
- 실행 직전 파일 존재, `.exe` 확장자, canonical 경로, 허용 설치 Root를 다시 확인합니다.

허용 Root는 `Program Files`, `Program Files (x86)`, `LocalAppData/Programs`, `Windows/System32`입니다.

## 차단 정책

- `shell.exec`, `cmd.exec`, `powershell.exec` 및 알려지지 않은 명령
- 전체 PowerShell·CMD 문자열
- 사용자가 전달한 실행 파일 경로
- 추가 프로세스 인자
- `cmd.exe`, PowerShell, WSH, MSHTA, Rundll32, Registry 편집기
- Temp, Downloads 등 허용 설치 Root 밖의 실행 파일
- 관리자 권한 자동 획득과 UAC 우회
- 2초 안에 반복된 동일 앱·동일 동작 요청

앱 종료는 검증된 Registry의 프로세스 이름에 대해 고정된 `taskkill /IM <name> /T` 형식만 사용하며 강제 종료 옵션은 사용하지 않습니다.

## 실행 로그

`native-execution.jsonl`에 다음 메타데이터만 기록합니다.

- Tool ID
- Rust 최종 위험도
- Registry 앱 이름
- 성공 여부
- 실행 시간
- 오류 코드
- 타임스탬프

전체 Shell 문자열, 실행 인자, 사용자 입력 원문은 기록하지 않습니다.
