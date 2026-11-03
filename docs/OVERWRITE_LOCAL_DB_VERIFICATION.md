# 실제 개발 앱 파일 실패 → 로컬 DB 검증

검증일: 2026-10-21. 검증 커밋: 미기록(기존 로컬 변경 포함). 사용자 수동 시험이 아니라 CLI가 실제 Windows ACE 개발 앱을 조작한 통합 검증이다.

## 환경과 방법

- 실제 `npm run tauri dev`의 메인 WebView2·AI 없는 Native 테스트 패널 사용. 외부 브라우저 모의 UI로 대체하지 않음. Windows Tauri의 WebView2는 Chrome 웹 테스트와 별개임.
- `start:native-test`의 로컬 백엔드 127.0.0.1:3001 사용. 로컬 DB 정책으로 localhost PostgreSQL만 허용하며 실제 인증·사용자 권한·실행 티켓·일회성 승인·Rust 대상 검사를 유지함. 운영 Render/Neon 접근 없음.
- 바탕화면의 이번에 생성한 `ACE-Native-Test-Trace-<UUID>` 폴더에 내용이 서로 다른 복사/이동 원본·대상만 생성함. 대상만 FileShare.None으로 열어 실제 Windows 배타 잠금을 유지함. 파일 ACL 변경·디스크 채우기 없음.
- 잠금 해제는 helper의 finally에서 FileStream.Dispose로 수행. 실패 전후 존재·SHA-256 비교, 새 요청 후 대상이 원본 내용과 같은지 확인함. 이동은 성공 후에만 원본이 사라지는지 확인함.
- 요청 전 잠금 시험과, 최종 승인 후 execute_native_tool 호출 직전 잠금 시험을 분리함. 후자는 CDP 디버거가 실제 invoke 함수 진입에서 잠시 멈추는 동안 잠금을 건 뒤 그대로 재개함. 응답·티켓·인자·승인 상태는 변경하지 않음.
- Native 감사 파일과 DB의 NativeExecution/NativeExecutionEvent를 ID로 대조함. DB는 상태·ID를 저장하고 오류 코드는 Native 감사 로그에서 확인함. DB에 오류 코드 필드가 저장된 것으로 확대하지 않음.

## 결과

| 조건 | 실제 중단 단계 | UI / Native / DB | 데이터 |
| --- | --- | --- | --- |
| 복사·이동 요청 전 대상 잠금 | prepare_native_tool의 파일 지문 준비 | UI `운영체제 권한이 거부되었습니다.` / execute_native_tool 호출 없음 / REQUESTED→FAILED | 두 파일 존재·해시 유지 |
| 복사·이동 승인 후 대상 잠금 | execute_native_tool → prepared_snapshot → 대상 read_locked, 실제 파일 열기 거부 | Native PERMISSION_DENIED·FAILED / 같은 executionId·attemptId의 DB FAILED / SUCCEEDED 없음 | 두 파일 존재·해시 유지 |
| 잠금 해제 후 복사 새 요청 | 새 권한 확인·승인·정상 Rust 실행 | UI 파일 복사 완료 / Native·DB SUCCEEDED | 원본 유지, 대상 해시=원본 해시 |
| 잠금 해제 후 이동 새 요청 | 새 권한 확인·승인·정상 Rust 실행 | UI 파일 이동 완료 / Native·DB SUCCEEDED | 대상 해시=원본 해시, 원본 제거 |

요청 전 잠금은 승인 준비 실패이며 Native 실행 감사 항목이 없는 것이 정상이다. 이 조건을 Rust 실행부터 DB까지 검증했다고 표시하지 않는다. 승인 후 잠금도 트랜잭션 쓰기 시작 전 재검증에서 거부되므로 **OS 잠금에 의한 쓰기 전 거부**이다. 이전 합성 실패+실제 TxF 롤백 검증과 구분한다.

### 최종 해시 증거를 저장한 실행

| 작업 | executionId | 최종 attemptId | Native·DB |
| --- | --- | --- | --- |
| 복사 실패 | `4a34062b-4edd-4da8-888f-984714dbfb15` | `5c8e4ef6-e601-4c88-b7ba-b67a8c0a3524` | FAILED / PERMISSION_DENIED |
| 복사 새 요청 성공 | `50379977-f783-4bc7-ac41-dc28f86af8fb` | `5b5227fc-ab2d-41c7-8324-5aa960f7fc1c` | SUCCEEDED |
| 이동 실패 | `3940dc40-4707-4afb-a2c0-4902cfc53ba4` | `8b9a3a1b-dd02-47e9-b095-c992263f0303` | FAILED / PERMISSION_DENIED |
| 이동 새 요청 성공 | `4a2d0975-3e31-4ee7-b5a8-a65b65159224` | `537d1dc5-0f7e-4828-993c-545ba717b2be` | SUCCEEDED |

- 복사 실패 전후 원본 SHA-256: `99854a262de2c86f2d7859f110ffe38d806329c9499d8c3f9bababd31795ed7f`
- 복사 실패 전후 대상 SHA-256: `6be8e89d5b0715f445d484a80feeb3e10afdede10703b3d6e4f55ae5ba19c20c`
- 이동 실패 전후 원본 SHA-256: `cc6b9f4b06d99dc669f5ad9cd85e8ef5448941ef4e3a3e5ea36ada82c4f4fe8b`
- 이동 실패 전후 대상 SHA-256: `78b96378ed82121d4846e3b34701dd8db4f057475e8cfaf2633738a4cb9f3edb`

최종 실패 시도에는 REQUESTED→WAITING_APPROVAL→APPROVED→RUNNING→FAILED가 존재하며 SUCCEEDED가 없음을 직접 조회했다. 새 요청의 requestId/executionId/attemptId는 실패 요청과 다르다. 새 요청은 기존 실패 시도의 자동 재실행이 아니다.

로컬 원문 증거: `.local/overwrite-db-result.json`, `.local/overwrite-db-first-result.json`, `.local/overwrite-audit-check.log`. 인증정보·파일 내용·전체 개인 파일 경로는 증거에 포함하지 않는다. 최초 도구의 invoke 교체는 읽기 전용 속성 때문에 적용되지 않아 잠금 타이밍 시험에 실패했으며 그 무잠금 성공은 검증 성공으로 계산하지 않았다. 이후 실제 디버거 정지 방식으로 검증했다.

## 범위와 남은 항목

- 이번 최종 실패의 DB 기록 누락·SUCCEEDED 오기록은 발견되지 않아 제품 코드 변경 없음. 실행·승인 정책 완화 없음.
- 충돌 탐색의 앞선 attempt는 Native APPROVAL_REQUIRED를 반환하고, 실제 덮어쓰기 승인에는 새 attemptId가 사용됨. 위 표는 최종 실행 시도의 실패/성공 연결을 검증한 결과이며 모든 중간 상태가 두 로그에서 같은 이름이라는 주장은 아님.
- DB/서버 단절 중 이벤트 저장 실패·재전송, 트랜잭션 쓰기 중 OS I/O/커밋 오류의 앱→DB 흐름, 비NTFS/다른 볼륨·설치 앱·운영 서버는 미검증 유지.
- 캡처 실제 UI 1시간 유지 시험은 대기 유지. 기존 사용자 수동 덮어쓰기 성공·취소·다른 이름·대상 변경 차단 통과와 기존 자동 검증은 변경 없음.
- 현재 요청 범위에 추가 사용자 조작은 필요 없음. 위험한 디스크 실패 시험을 요구하지 않음. 커밋·푸시·배포 없음.
