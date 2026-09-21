# Wake Word 및 음성 녹음

## 엔진 선택

ACE는 Windows의 `Windows.Media.SpeechRecognition` API와 `SpeechRecognitionListConstraint`를 사용한다. 인식 문법은 `ACE` 한 단어로 제한되며 원시 오디오는 백엔드, 데이터베이스 또는 클라우드로 전송하지 않는다.

- Windows 지원: Windows 10/11 내장 API
- Rust/Tauri 통합: `windows` crate 0.61
- 사용자 지정 키워드: 목록 문법에 `ACE` 등록
- 처리 위치: 설치된 Windows 음성 언어 팩을 이용한 로컬 제한 문법 인식
- 라이선스: Windows 시스템 API이며 별도 상용 SDK 키가 필요하지 않음
- 배포 파일: 별도 Wake Word 모델 파일 없음. Windows 음성 인식 언어 구성 요소가 필요함
- 부하 제어: 전체 받아쓰기 대신 한 단어 목록 문법만 연속 실행

Windows에 음성 인식 구성 요소가 없거나 마이크 권한이 거부되면 상태를 `Error`로 전환하고 설정 화면에 오류를 전달한다.

## 상태와 호출 흐름

`Disabled → Starting → Listening → Triggered → Listening` 상태를 사용한다. 설정과 트레이는 같은 Rust 런타임을 제어한다. `ACE`가 High 또는 Medium 신뢰도로 감지될 때만 기존 `activate_voice_orb()` 흐름을 호출한다.

동일 감지는 3초 동안 무시하고, Orb가 이미 활성 상태이면 새 세션을 만들지 않는다. 트레이의 `음성 호출`은 Wake Word가 꺼져 있어도 같은 Orb 진입점을 직접 호출한다.

## 녹음 파일 정책

- 활성 녹음은 `recordings/*.wav.part` 임시 파일로 작성한다.
- 사용자가 `녹음 종료 · WAV 저장`을 선택한 경우에만 헤더를 완성하고 `.wav`로 변경한다.
- 명령 실행, 취소, Orb 닫기, 앱 종료 시 임시 파일을 삭제한다.
- 다음 녹음을 시작할 때 비정상 종료로 남은 `.part` 파일을 정리한다.
- 최대 녹음 시간은 30분이며 PCM 샘플의 유한값과 청크 크기를 계속 검증한다.

## 운영 확인

실제 장치에서는 작업 관리자에서 유휴 상태의 ACE CPU와 메모리를 5분간 관찰한 뒤 Wake Word ON 상태를 같은 시간 동안 비교한다. 마이크 드라이버와 Windows 언어 팩에 따라 수치가 달라지므로 배포 대상 PC에서 확인한다.
