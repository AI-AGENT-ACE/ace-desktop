# 기본 호출어 모델·입력 마이크·앱 창 선택

## 현재 완료 범위

- 공용 모델의 설치 리소스 패키징·설치 경로 로드·Release 차단 기반 구현
- 공용 모델 없음: Release 빌드는 실패하는 것이 정상이며, 정확도 검증 완료를 의미하지 않음
- 공용 모델 수집·동의·다른 사람의 실시간 호출 평가·새 PC 설치 평가는 사용자가 진행할 작업
- 개발 모드는 기존 개인 모델과 `ACE_DEFAULT_WAKE_MODEL` 실험 경로를 계속 사용 가능
- 이번 변경은 커밋·배포·버전 변경 없이 로컬에 적용

## 저장 위치와 배포 검사

- 공용 리소스 원본: `src-tauri/resources/wake-word/`
- 설치 후 공용 모델: `app.path().resource_dir()/wake-word/manifest.json`에 선언된 파일
- 개인 모델: `app.path().app_config_dir()/wake-word/user-reference.rpw`
- 마이크 설정: `app.path().app_config_dir()/audio-input.json`
- 공용 모델은 설치 폴더에서 읽기만 하고 개인 모델에 복사하지 않음
- 앱 업데이트는 설치 리소스를 바꾸며 사용자 설정 폴더의 개인 모델을 덮어쓰지 않음
- 최초 호출어 감지는 기존처럼 사용자가 설정에서 켜는 방식 유지. 개인 녹음은 필수 아님
- Release에서는 개인 모델 환경변수와 개발 폴더를 기본 모델로 사용하지 않음

검사 명령은 프로젝트 폴더에서 실행한다.

```powershell
node scripts/check-wake-model.mjs
npm run tauri build
```

Tauri Release 시작 전 검사하고, 직접 Cargo Release를 실행해도 build.rs에서 다시 검사한다.
검사 항목: 파일 존재·읽기 가능 여부·1 KiB~16 MiB 크기·선언 크기·SHA-256·CBOR 구조·ACE 이름·13 MFCC 특성·유한한 수치·엔진 및 모델 버전·재배포 승인·평가 기록.
검사기는 기록의 형식과 선언된 기준을 검사한다. 실제 동의 취득이나 실제 호출 정확도를 자동으로 증명하지 않는다.
공용 모델이 준비되기 전 기존 CI의 Release 빌드 단계 역시 의도적으로 실패한다. 이를 피하기 위한 빈 모델·개인 모델·가짜 기록을 넣지 않는다.

## 사용자가 준비할 공용 모델

1. 녹음 참여자 동의를 먼저 받는다.
   - 녹음 수집·보관·모델 및 MFCC 특징 생성·성능 평가·공개 설치 파일에 모델 및 파생 특징 재배포 허용 범위를 명시
   - 원본 음성 공개는 별도 동의이며 기본적으로 공개하지 않음
   - 공개 배포된 파일은 회수가 어려움을 안내하고, 철회 시 향후 모델 교체·배포 중단 범위를 설명
   - 서명된 동의서·실명·원본 음성은 비공개로 보관하고 Git에 포함하지 않음
2. 학습 참여자와 평가 참여자를 나눈다.
   - 시작 기준으로 학습 참여자 최소 3명, 가능하면 5명 이상 권장
   - 각자 자연스럽게 “에이스”를 5~10회 녹음
   - 평가 참여자는 학습에 사용하지 않은 다른 사람 최소 3명
   - 연령·목소리 높낮이·거리·마이크가 다른 조건을 포함. 특정 개인에게만 맞는 모델을 공용으로 지정하지 않음
3. WAV를 준비한다.
   - mono, 16 kHz, signed PCM 16-bit WAV
   - 파일 하나에 호출어 한 번, 앞뒤 약 0.2~0.3초 여유, 전체 0.35~3.5초
   - 소리가 잘리거나 심하게 찢어진 파일·무음 파일 제외
   - 익명 고유 이름 사용: `p01_01.wav`, `p01_02.wav`, `p02_01.wav`
   - 모든 파일을 비공개 학습 폴더에 모으고 같은 파일명을 재사용하지 않음
4. 후보 모델 생성한다.

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --example build_public_wake -- C:/private-ace-training C:/private-ace-models/ace-candidate.rpw
```

현재 엔진은 Rustpotter 3.0.2의 reference/DTW 방식이다. 여러 사람의 샘플을 참조 특성으로 구성하며 범용 정확도가 보장되는 사전학습 모델은 아니다.
도구는 WAV 형식·길이·무음·클리핑을 검사하고 후보를 만들지만 재배포 동의나 성능 기록을 생성하지 않는다.

5. 실제 입력으로 후보를 평가한다.

```powershell
$env:ACE_DEFAULT_WAKE_MODEL = 'C:/private-ace-models/ace-candidate.rpw'
npm run tauri dev
```

개인 모델을 우선 사용 중이면 후보 평가에 별도 테스트 프로필을 사용한다. 개인 모델을 삭제하거나 덮어쓰지 않는다.
개발 로그의 `model_id`와 후보 SHA-256을 대조해 실제 시험한 모델을 확인한다.

- 각 평가 참여자가 10회 이상 호출하고 시도 수·Orb가 열린 횟수·거리·마이크·민감도·코드 버전을 기록
- 매 시도 후 Orb를 닫고 호출 쿨다운 3초보다 길게 기다린 후 다음 호출
- 기본 감도와 민감한 감도를 각각 시험. 배포 게이트의 수치는 기본 감도 결과로 기록
- 호출어가 없는 대화·비슷한 발음·TV·음악·키보드·무음 등 합계 1시간 이상 시험하고 의도하지 않은 Orb 활성화 횟수 기록
- 시작 게이트: 평가 참여자 3명 이상, 사람별 10회 이상, 사람별 성공률 90% 이상, 음성 없는 평가 1시간 이상, 오탐 시간당 1회 이하
- 이는 초기 배포 기준이며 품질 보증 수치가 아님. 다양한 조건에서 평가 시간을 늘리고 실패 조건을 기록
- 녹음 재생 시험은 보조 자료이며 실제 사람의 마이크 호출 평가를 대신하지 않음

보조 재생 시험:

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --example evaluate_public_wake -- C:/private-ace-models/ace-candidate.rpw C:/private-ace-evaluation/p06_01.wav
```

6. 동의와 평가가 완료된 뒤에만 배포 폴더에 모델·manifest·익명 기록을 넣는다.

`manifest.json` 필드:

- `formatVersion`: 1
- `modelVersion`: 예를 들어 `1.0.0`처럼 3자리 숫자 버전
- `engine`: `rustpotter-3.0.2-ref`
- `file`: `ace.rpw` 고정. 이 파일만 Git의 개인 모델 제외 규칙에서 예외로 허용
- `sizeBytes`: 실제 파일 크기
- `sha256`: 실제 모델의 소문자 SHA-256
- `redistributionApproved`: 실제 승인 완료 후 true
- `consentRecord`: 익명 승인 요약 JSON의 상대 경로
- `evaluationRecord`: 실제 평가 요약 JSON의 상대 경로

승인 요약 필드:

- `redistributionApproved`: 실제 승인 완료 여부
- `scope`: `model-and-derived-features`
- `recordReference`: 비공개 동의 기록을 연결할 익명 관리 번호. 이름이나 연락처 제외

평가 요약 필드:

- `modelSha256`: 평가한 모델의 실제 SHA-256
- `heldOutSpeakers`: 학습에 포함되지 않은 평가자별 `participantId`, `attempts`, `detections` 배열
- `negativeAudioSeconds`: 호출어 없는 실제 평가 시간
- `falseActivations`: 의도하지 않은 실제 활성화 횟수

평가 상세 기록에는 별도로 날짜·코드 버전·감도·장치·거리·조건·실패 사례를 보관한다. 코드의 합성 단위 테스트 파일을 실제 평가 기록으로 사용하지 않는다.

## 마이크 설정·실기기 시험

- 설정 → 입력 마이크에서 시스템 기본 또는 개별 장치 선택
- 녹음·호출어 감지·개인 음성 등록이 동일한 CPAL 장치 ID 설정을 사용
- 장치 변경 시 기존 스트림을 종료하고 미완성 녹음을 취소한 후 새 장치 적용
- 음량 확인 버튼으로 입력 meter 확인. 원본 샘플을 저장하지 않음
- 기본 마이크 자동 대체는 기본적으로 꺼져 있음
- 자동 대체를 켜면 선택 장치가 없을 때 다음 입력 시작에서 시스템 기본 장치를 사용
- 녹음 도중 장치가 끊기면 해당 입력은 중지·취소. 중간에 다른 마이크의 소리를 이어 붙이거나 불완전 녹음을 업로드하지 않음
- 호출어 스트림 오류 후에는 설정에서 장치를 다시 선택해 재시작

사용자가 확인할 순서:

1. USB 마이크 선택 → 음량 표시 확인
2. USB 마이크로 호출어 감지와 Orb 음성 명령 확인
3. 녹음 도중 USB 연결 해제 → 중지·안내 확인, 업로드되지 않는지 확인
4. 설정에서 다른 마이크 선택 → 앱 재시작 없이 다시 호출·녹음 확인
5. 자동 대체를 켜고 선택 장치를 제거 → 다음 입력에서 시스템 기본 장치 사용 확인
6. 자동 대체를 끈 상태로 동일 시험 → 임의 장치 사용 없이 재선택 안내 확인

## 앱 창 선택·실기기 시험

- 앱 실행을 제외한 종료·포커스·최소화·최대화는 실제 표시 중인 창을 조회
- 같은 앱의 창이 여러 개면 앱 이름·창 제목 선택지를 표시
- 메인 채팅 요청은 메인 창의 선택 UI, Orb 요청은 Orb 선택 UI 사용
- 창 핸들·PID·프로세스 시작 시각·경로·창 수명 표시값은 Rust에서만 관리
- UI에는 2분 만료·일회성 대상 ID만 전달. 선택한 작업과 프로세스에 결합해 검사
- 실행 직전에 창 존재·프로세스·시작 시각·창 수명 표시값 재검증
- 창이 사라지거나 ID가 만료·변조되면 다른 창을 선택하지 않으며 다시 조회하도록 안내
- 종료는 선택한 창에 WM_CLOSE를 요청. 프로세스 전체 종료나 강제 종료를 하지 않음
- 미저장 문서의 저장 확인창은 앱이 처리. 종료 요청 전달은 실제 앱 종료 완료와 다름
- 하나의 창 안에 있는 여러 탭은 Windows 창 선택 대상이 아님
- Windows가 전경 창 전환을 거부하면 포커스 요청은 권한 오류로 반환. 다른 창에 실행하거나 전경 창 보호 정책을 해제하지 않음

사용자가 확인할 순서:

1. 같은 앱의 독립된 창 3개를 서로 다른 제목으로 열기
2. 최소화·포커스·최대화 요청 → 각각 선택한 창에만 적용되는지 확인
3. 종료 요청 → 선택한 창만 종료되는지 확인
4. 미저장 내용이 있는 창 종료 → 앱의 저장 확인이 유지되는지 확인
5. 선택지를 띄운 뒤 대상 창을 닫기 → 클릭해도 다른 창에 실행되지 않고 다시 조회 안내가 나오는지 확인
6. 2분 후 선택 → 만료 안내 확인
7. 메인 채팅과 Orb에서 모두 시험

## 새 PC 최종 완료 조건

- 실제 공용 모델·동의·여러 사람 평가 완료 후 Release 검사·설치 파일 생성
- 개발 폴더와 개인 모델이 없는 Windows PC에 설치
- 입력 마이크를 설정하고 음성 호출을 켠 뒤 개인 녹음 없이 호출 확인
- 개인 모델 등록 후 앱 업데이트 → 개인 모델 파일·설정 보존 확인
- 이 시험 전에는 기본 모델 배포의 최종 완료로 표시하지 않음

## 이번 변경 검증 기록

- React production 빌드와 Rust debug 앱 빌드 통과
- Rust 일반 테스트 64개 통과, 실기기·개인 음성·GUI 관련 8개 기본 제외
- 제외된 GUI 테스트 1개를 별도로 실행해 선택한 창만 최소화·포커스·종료되는 흐름과 사라진 대상 거부 확인
- Windows가 포커스를 거부하는 경우도 확인했으며, 테스트는 정상 전환 또는 명시적 권한 거부를 구분해 검증
- Orb·호출어 설정·음성 등록·음성 회귀·Markdown JavaScript 테스트 합계 28개 통과
- 브라우저 UI: 창 선택·사라진 창 재조회·마이크 설정 3개와 Orb 표시 2개 통과
- cargo clippy와 변경 파일 포맷 검사 통과
- 공용 모델이 없는 Release는 `ACE_PUBLIC_WAKE_MODEL_INVALID` / `ACE_DEFAULT_MODEL_MISSING` 오류로 중단 확인
- 전체 기존 Prettier 검사에는 변경 전부터 있던 파일 6개의 경고가 남음: AuthForm.tsx, orbMotion.ts, WakeDiagnosticPanel.tsx, WakeWordSetupModal.tsx, test-wake-setup.mjs, tray.spec.ts
- 실제 USB 연결 해제·여러 사람의 실제 호출·새 PC 설치·개인 모델 유지 업데이트는 아직 실행하지 않음

구현 참고: [Tauri 리소스 패키징](https://v2.tauri.app/develop/resources/), [Windows WM_CLOSE](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-close), [Windows 전경 창 전환 제한](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow).
