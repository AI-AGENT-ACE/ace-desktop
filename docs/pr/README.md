# 음성 수정 PR 진행 방법

변경은 세 개의 의존 브랜치로 나눴다. 2026-10-06에 기존 원격 브랜치로 한국어 PR을 생성했다. 사용자 요청에 따라 자동 검사와 충돌 여부를 확인한 뒤 순서대로 main에 병합한다. 실제 마이크·Windows 화면 동작은 자동 검사와 별개이며, 미검증 범위는 각 PR에 명시한다.

| 순서 | head | base | 본문 |
|---|---|---|---|
| 1 | fix/wake-native-enrollment | main | [Wake 등록 수정](01-wake-native-enrollment.md) |
| 2 | feat/local-voice-portfolio | fix/wake-native-enrollment | [로컬 음성 처리](02-local-voice-portfolio.md) |
| 3 | fix/voice-orb-session | feat/local-voice-portfolio | [오브 세션 개선](03-voice-orb-session.md) |

## 현재 업로드 상태

이전 환경의 프록시 오류 기록과 달리 현재 세 작업 브랜치는 원격에 존재한다. GitHub CLI 인증과 원격 연결도 확인했다. 샌드박스의 Git 인증 오류는 정식 실행 승인을 받아 조회했다. 이미 생성한 아래 PR을 사용하며 중복 생성하지 않는다.

- [#19: 등록 입력 통일과 실제 호출 검증](https://github.com/AI-AGENT-ACE/ace-desktop/pull/19)
- [#20: 세션 복구와 로컬 포트폴리오 음성 명령](https://github.com/AI-AGENT-ACE/ace-desktop/pull/20)
- [#21: 오브 표시와 재시도·확인 흐름 개선](https://github.com/AI-AGENT-ACE/ace-desktop/pull/21)

표의 base는 PR 생성 당시 기준이다. 앞 PR을 병합한 뒤 다음 PR의 base를 main으로 옮기고 비교 diff와 CI를 확인한다. 선행 커밋을 보존하는 merge commit 방식으로 병합한다. 최종 상태와 검사 결과는 링크된 PR을 기준으로 확인한다. Git 명령은 ACE 상위 폴더에서 `git -C ./ace-desktop`으로 실행하고, gh 명령은 해당 저장소를 작업 디렉터리로 사용한다.

커밋은 `fix(scope): 한국어 설명`, `feat(scope): 한국어 설명`, `docs(scope): 한국어 설명` 형식을 따른다. Git 작성자 이름/이메일은 이 저장소에만 설정했다.
