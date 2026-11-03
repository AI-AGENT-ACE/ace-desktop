# Native 기본 명령 24개 — 구현과 검증 기록

2026-10-13. 아래는 코드 구현 여부이며 설치 앱·실기기 시험 완료 표기가 아님. 기존 로컬 변경을 보존했고 커밋·푸시·배포하지 않음.

## 발견한 문제와 수정

- Rust 실행 분기와 달리 NestJS 카탈로그에는 app.open/close, file.open/rename/delete만 등록되어 있었음. 따라서 file.copy와 screen.capture가 AI 응답에서 지원하지 않는 도구로 처리됨.
- 실제 AI 서버의 텍스트 응답은 파일 복사에 query/번역 후보/extensions/destination, 삭제에 query/extensions를 사용했음. 직접 경로 인자와 달라 삭제도 인자 검증 실패가 발생할 수 있는 상태였음.
- NestJS 외부 도구를 LOCAL 명령 24개로 등록하고 입력·결과 JSON Schema, 기본 권한과 시스템 승인 여부를 정의함. weather.current, 임의 Shell, 강제 종료, 영구 삭제는 이 목록에 없음.
- AI의 파일 검색 의도를 로컬 파일 검색 → 사용자 원본 선택 → 대상 위치/파일명 확인 → resourceId 기반 실행으로 연결함. 유일한 검색 결과도 자동 실행하지 않음. 검색 위치와 대상 경로는 사용자가 확인하며 AI 문장에서 임의의 경로를 추측해 실행하지 않음.
- 검색 결과에 허용된 폴더 기준 상대 경로를 표시하고 main에서 검색 결과 열기 제공. 검색 및 결과 열기·캡처 이미지 열기 전에 최신 도구 권한 조회. DENY는 차단, ASK는 확인 UI 사용.
- 잘못된 AI 인자는 티켓을 발급하지 않고 아무 작업도 실행하지 않았다는 채팅 안내를 저장·반환함. 파일·작업 원문을 운영 로그에 출력하지 않음.
- Rust Confirm 명령은 confirmed:true 외에 명령·전체 인자·파일 지문·만료에 결합된 일회성 승인 ID가 있어야 실행함. 덮어쓰기에는 기존의 별도 교체 승인 토큰도 필요함.
- Native 실행 ID는 같은 프로세스에서 재사용 불가. 기록은 만료로 제거하지 않고 최대 65,536개에서 안전하게 거부함. signed call ID를 requestId로 전달해 Native 감사 로그와 서버 티켓의 실행 단위를 연결함.
- 서버는 서명된 call ID를 ToolExecutionLog.executionId 고유 키로 소비해 결과 중복 반영을 거부함. 기존 레코드와의 호환을 위해 컬럼은 nullable. 동일 결과 재요청으로 AI를 다시 호출하거나 TOOL 메시지를 중복 저장하지 않음.

## 코드 구현 상태

모든 행에 NestJS 입력/결과 계약·LOCAL 분류·권한 처리와 Rust 실행 분기가 있음. 실기기 검증 상태는 별도 표 참조.

| 분류        | 명령                                                        | 기본 정책 / Rust 확인                         |
| ----------- | ----------------------------------------------------------- | --------------------------------------------- |
| Application | app.open                                                    | ALWAYS_ALLOW / Safe                           |
| Application | app.close                                                   | ASK / Confirm                                 |
| Application | app.focus, app.minimize, app.maximize                       | ALWAYS_ALLOW / Safe                           |
| File        | file.search, file.open                                      | ALWAYS_ALLOW / Safe                           |
| File        | file.rename, file.move, file.copy, file.delete              | ASK / Confirm                                 |
| Folder      | folder.open, folder.search                                  | ALWAYS_ALLOW / Safe                           |
| Folder      | folder.create                                               | ASK / Confirm                                 |
| Web         | web.open, web.search                                        | ALWAYS_ALLOW / Safe                           |
| Clipboard   | clipboard.write                                             | ALWAYS_ALLOW / Safe                           |
| Clipboard   | clipboard.read                                              | ASK / Confirm                                 |
| Voice       | voice.activate                                              | ALWAYS_ALLOW / Safe                           |
| Screen      | screen.capture                                              | ALWAYS_ALLOW / Safe + 사용자 모니터/영역 선택 |
| System      | system.volume.set, system.volume.mute, system.open_settings | ALWAYS_ALLOW / Safe                           |
| System      | system.lock                                                 | ASK / Confirm                                 |

사용자가 설정한 DENY/ASK도 적용. Rust의 Confirm/Blocked를 서버 ALWAYS_ALLOW로 완화하지 않음. 이전 system.status는 로컬 진단 호환 명령으로 남아 있지만 NestJS 외부 24개 목록에는 없음.

## 기능별 보완

- 앱: 기존 창 ID → HWND/프로세스/시작 시각/창 nonce 검증 유지. 선택한 창에 WM_CLOSE 요청. 요청·실제 종료를 구분하며 5초 뒤 창이 남으면 저장 확인 또는 사용자 조작 안내. 강제 종료 없음.
- 파일: Resource ID 만료와 실제 파일 ID·크기·변경 시각 재검증, 심볼릭 링크/재분석 지점 거부. 일반 경로는 사용자 허용 폴더에서만 처리. 덮어쓰기는 지원되는 같은 볼륨의 트랜잭션 교체만 허용하며 실패 시 롤백. 휴지통 실패 시 영구 삭제 전환 없음.
- 폴더: 알려진 부모 폴더, 검색한 부모 resourceId 또는 허용 폴더 아래 parentPath 지원. 이름·부모 대상·중복 검증. 승인 발급/실행 시 부모 상태 재검사.
- 웹: HTTP/HTTPS만 허용하고 URL을 별도 프로세스 인자로 전달. 웹 검색은 google/bing/duckduckgo만 사용하며 q를 URL 인코딩. 임의 Shell 명령 문자열을 생성하지 않음.
- 클립보드: main HWND를 소유자로 사용해 텍스트 쓰기. 실패 시 할당 메모리 해제. 읽기는 메모리 크기·UTF-16·최대 UTF-8 크기를 검사. 읽은 텍스트는 앱의 로컬 보기 창에서만 표시하고 서버·AI에는 요약 완료 표시만 전송. 내용을 운영 로그에 저장하지 않음.
- 음성 활성화: 선택된 마이크 조회·기본 입력 구성 확인 후 Orb 활성화. 단순 활성화 요청을 LISTENING 완료로 취급하지 않고 실제 입력 스트림 준비를 기다림. 녹음·인식·도구 실행·승인·발화 처리 중 재활성화는 중복 입력을 시작하지 않음. 답변을 표시한 채 대기 중인 Orb에는 새 입력 요청 가능.
- 캡처: AI 요청은 빈 인자로 받고 실제 모니터 ID·full/region·선택 영역은 전용 로컬 IPC에서 받음. 선택한 모니터 내부에서 배율과 음수 원점을 계산. 제외 설정·선택창 숨김 후 캡처. 취소·실패 시 소유 파일 핸들로 임시 이미지만 정리하며 사용자가 저장한 파일은 삭제하지 않음.
- Windows 설정: 빈 값(설정 홈), display/sound/bluetooth/network/appsfeatures/privacy-microphone/privacy-webcam/windowsupdate/personalization/notifications/about만 허용. 임의 ms-settings URI 조합 거부.

## JSON 계약 위치

- [외부 24개 및 내부 IPC JSON 계약](native-tool-contract.json)
- `GET /tools`도 동일한 입력/결과 Schema와 권한 메타데이터 제공.
- NestJS 카탈로그가 생성 원본이며 backend와 desktop의 JSON 파일 내용은 동일하게 내보냄. Rust 검사에서 24개 이름·LOCAL 분류·시스템 확인과 Rust 정책의 일치 검증.
- 외부 AI 인자와 로컬 확정 인자를 구분. overwriteToken/approvalId/targetWindowId/monitorId/sessionId는 로컬에서 발급·관리. AI가 교체 승인 토큰이나 캡처 모니터 ID를 만들어 전달하면 외부 입력 검증에서 거부.
- Native 결과: requestId, executionId, tool, success, status, data, error. status는 SUCCEEDED/FAILED/SELECTION_REQUIRED/APPROVAL_REQUIRED/CANCELLED/REQUESTED. 선택·승인 대기는 최종 결과로 서버에 보고하지 않고 로컬 흐름을 유지함.

계약 다시 생성(ace-backend에서):

```powershell
npm run build
node scripts/export-native-contract.cjs --desktop
```

## 검증과 미검증 구분

| 검사                       | 상태                                                                                                           |
| -------------------------- | -------------------------------------------------------------------------------------------------------------- |
| NestJS 전체 단위·통합 검사 | 122개 통과. 외부 AI·DB는 모의 처리한 검사 포함                                                                 |
| 24개 전달 계약             | 실제 HTTP AI 어댑터 → 실제 카탈로그/권한 → 서명 티켓 발급·소유자 검사 → 반환 흐름 검사                         |
| Rust 일반 검사             | 79개 통과, 11개 명시적 제외. 실음성·GUI 제외 항목을 완료로 처리하지 않음                                       |
| 브라우저 UI                | 14개 통과. AI 파일 의도 → 검색·선택 → 별도 승인·덮어쓰기, 삭제, 캡처 선택 등 Native 응답 모의 검사             |
| JS 회귀                    | 26개 통과. 음성 입력 없는 Orb/호출어/Markdown 모의 검사                                                        |
| 빌드·정적 검사             | NestJS·React production 및 Rust debug 빌드, Clippy 통과                                                        |
| 실제 AI 서버 계약 확인     | 테스트용 텍스트 3개로 copy/delete/capture 응답 형태 확인. Windows 작업은 실행하지 않고 이번에 만든 대화는 정리 |
| 실제 Windows 작업          | 앞선 4~7번 검사에서 테스트 전용 파일 교체/롤백, 휴지통 이동, 테스트 창 종료 관찰, 캡처 선택창 제외 검사 통과   |
| 수동 검증 대기             | 실제 앱별 24개 정상·실패 시나리오 전체, 다중 모니터·DPI·물리 연결 해제, 실음성/USB 마이크/새 PC 설치           |

이전 1~3번의 미실행 음성 테스트는 그대로 대기. 공용 Wake Word 모델·동의·여러 사람 평가가 없어 Release는 계속 차단됨. 이번 작업은 실제 음성 시험을 실행하지 않음.

## 서버와 설치 앱 반영 상태

- Render 운영 서버와 기존 설치 앱에는 이번 로컬 변경이 아직 반영되지 않음. 개발 앱만 다시 켜고 이전 Render 서버를 사용하면 여전히 지원하지 않는 도구 응답이 나올 수 있음.
- 백엔드 반영 전에 `202610130001_native_execution_receipt` migration 필요. Render의 기존 start:render는 migrate deploy 성공 후 서버를 시작하는 구성. 원격 DB 접속·migration 적용은 하지 않음.
- `.env`, JWT 서명 비밀키, DB URL은 데스크톱에 추가하지 않음. AI 텍스트 시험에는 가상 파일 요청만 사용.

## 파일 테스트 방법 변경

1. 바탕화면에 ACE테스트 폴더와 작은 원본.txt/대상.txt 준비.
2. 메인 채팅에 복사 요청 → 허용 → 파일 대상 확인.
3. 찾을 위치=바탕화면, 검색어=원본.txt → 파일 찾기 → ACE테스트/원본.txt 선택.
4. 대상 위치=바탕화면, 대상 경로=ACE테스트/대상.txt 입력 후 진행.
5. 충돌 창에서 취소 / 다른 이름 / 덮어쓰기 확인. 중요 파일을 사용하지 않음.
6. 삭제 요청도 실제 검색 결과를 선택한 뒤 휴지통 이동. Native 안전 동작의 자세한 시험은 [NATIVE_SAFETY_AND_CAPTURE.md](NATIVE_SAFETY_AND_CAPTURE.md) 참조.
