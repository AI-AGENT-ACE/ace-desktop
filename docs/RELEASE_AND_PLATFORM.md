# Desktop Release와 플랫폼 정책

## 지원 범위

- **공식 지원:** Windows 10/11 x64
- **미지원·미검증:** macOS, Linux

Registry 탐색, Windows 앱 실행, 작업표시줄 AppUserModelID, Windows 음성 인식과 Installer가 Windows API에 의존한다. 현재는 억지로 다른 OS까지 추상화하지 않으며 Rust의 `cfg(windows)`와 기능 모듈 경계를 유지한다. 다른 OS 지원은 tray, path, permission, launcher, packaging, autostart, global shortcut을 별도 요구사항으로 검증한 뒤 선언한다.

## CI와 Release

Pull Request와 `main` push에서 Windows Runner가 다음을 검사한다.

1. npm 고정 의존성 설치와 Prettier
2. TypeScript·Vite production build
3. Rust format, test, Clippy
4. 번들을 제외한 Tauri production binary build

Release는 `vMAJOR.MINOR.PATCH` 태그 또는 수동 workflow로 실행한다. `package.json`과 `src-tauri/tauri.conf.json` 버전이 태그와 같고 Repository Variable `PRODUCTION_API_URL`이 HTTPS여야 한다. 성공하면 Tauri NSIS `.exe`를 Workflow Artifact와 GitHub Release에 올린다.

```powershell
# 예: 두 버전을 0.2.0으로 수정하고 검증한 뒤
git tag v0.2.0
git push origin v0.2.0
```

Release 전 다음을 확인한다.

- production API URL과 Tauri CSP
- debug logging과 dev tools 비활성 상태
- npm·Tauri 앱 버전 일치
- ACE icon과 Installer metadata
- Wake Word의 Windows 음성 언어 구성 요소·마이크 권한
- 새 dependency와 이미지·폰트의 재배포 라이선스
- Windows code signing 적용 여부

현재 workflow는 Windows code signing 인증서를 요구하지 않는다. 미서명 Installer에는 SmartScreen 경고가 나타날 수 있으며 학교·포트폴리오 배포의 한계로 문서에 표시한다. 인증서를 도입할 때 private certificate와 password는 GitHub Production Environment Secret으로만 제공한다.

## 라이선스

저장소의 프로젝트 라이선스는 소유자가 결정하기 전까지 생성하지 않는다. Release 전에 npm package metadata와 Cargo dependency license를 검토하고, Wake Word는 별도 모델을 포함하지 않고 Windows 시스템 음성 인식 API를 사용한다는 점을 확인한다.
