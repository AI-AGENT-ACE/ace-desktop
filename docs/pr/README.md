# 음성 수정 PR 진행 방법

현재 변경은 세 개의 의존 브랜치로 커밋했다. main은 병합하지 않았다. 한 번에 큰 PR로 묶기보다, 각 사건의 원인과 결과를 검토할 수 있게 Draft PR 세 개로 분리한다. 실제 음성/시각 검증과 CI를 확인한 뒤 순서대로 병합한다.

| 순서 | head | base | 본문 |
|---|---|---|---|
| 1 | fix/wake-native-enrollment | main | [Wake 등록 수정](01-wake-native-enrollment.md) |
| 2 | feat/local-voice-portfolio | fix/wake-native-enrollment | [로컬 음성 처리](02-local-voice-portfolio.md) |
| 3 | fix/voice-orb-session | feat/local-voice-portfolio | [오브 세션 개선](03-voice-orb-session.md) |

## 현재 업로드 상태

개발 도구에서 GitHub push가 프록시의 443 연결 실패로 중단됐다. **원격 브랜치 업로드와 PR 생성은 완료하지 않았다.** GitHub CLI도 이 환경에 설치되어 있지 않다. 다음 명령은 네트워크와 GitHub 인증이 가능한 환경에서 사용하는 후속 절차다. 현재 브랜치에서 이 문서들과 PR 본문을 볼 수 있다.

```powershell
git push -u origin fix/wake-native-enrollment
git push -u origin feat/local-voice-portfolio
git push -u origin fix/voice-orb-session

gh pr create --draft --base main --head fix/wake-native-enrollment --title 'fix(wake): 등록 입력 통일과 실제 호출 검증' --body-file docs/pr/01-wake-native-enrollment.md
gh pr create --draft --base fix/wake-native-enrollment --head feat/local-voice-portfolio --title 'feat(voice): 세션 복구와 로컬 포트폴리오 음성 명령' --body-file docs/pr/02-local-voice-portfolio.md
gh pr create --draft --base feat/local-voice-portfolio --head fix/voice-orb-session --title 'fix(voice): 오브 표시와 재시도·확인 흐름 개선' --body-file docs/pr/03-voice-orb-session.md
```

CLI가 없으면 GitHub 웹에서 위 base/head를 선택하고 해당 본문을 사용한다. 앞 PR을 병합할 때 다음 PR의 base를 main으로 옮기고 비교 diff를 확인한다. 선행 커밋을 보존하는 merge 방식을 쓰면 의존 관계 관리가 쉽다. squash/rebase merge를 선택하면 후속 브랜치도 새 기준에 맞춰 rebase해야 하므로 자동으로 main에 합치지 않는다.

커밋은 `fix(scope): 한국어 설명`, `feat(scope): 한국어 설명`, `docs(scope): 한국어 설명` 형식을 따른다. Git 작성자 이름/이메일은 이 저장소에만 설정했다.
