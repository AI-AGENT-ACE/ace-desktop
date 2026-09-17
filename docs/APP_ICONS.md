# ACE 작업표시줄·트레이 아이콘

원본은 `public/ace-logo.png`이며 이미 투명 배경을 갖고 있습니다. 가로형 로고의 비율을 유지해 1024×1024 투명 캔버스 중앙에 배치한 `src-tauri/icons/ace-source.png`를 만들었습니다. 처음 추가했던 흰 배경은 제거했고 원본의 알파 채널을 유지해 아이콘을 다시 생성했습니다.

로고를 더 크게 보이게 하기 위해 원본의 투명 여백을 제외한 사각형 `(58, 116, 1556, 663)`을 사용합니다. 이를 1024×1024 캔버스에서 가로 992px로 배치해 좌우 여백을 16px씩 남겼습니다. 이전 아이콘 대비 글자가 약 13% 크게 표시되며 원본 파일은 유지합니다.

아래 명령으로 Windows ICO와 크기별 PNG, 다른 플랫폼 아이콘을 생성했습니다.

```powershell
npm run tauri -- icon src-tauri/icons/ace-source.png --output src-tauri/icons
```

`src-tauri/tauri.conf.json`의 기존 `bundle.icon` 경로에 생성 파일을 덮어썼습니다. `icon.ico`는 Windows 실행 파일과 배포 아이콘에 사용됩니다. Rust `src-tauri/src/lib.rs`에서는 `app.default_window_icon()`을 실제 창의 `set_icon()`과 트레이의 `icon()`에 동일하게 적용합니다.

네이티브 아이콘은 앱 재시작이 필요합니다. X 버튼은 앱을 숨기므로 트레이의 **ACE 종료**로 종료한 뒤 실행해야 합니다. 개발 중에는 Ctrl+C 후 `npm run tauri dev`로 다시 실행합니다. 기존 작업표시줄 고정 바로가기가 이전 아이콘을 표시한다면 고정을 해제하고 새 실행 파일을 다시 고정하세요.

검증: `cargo build --offline`로 Windows 네이티브 빌드를 확인합니다.

## 작업표시줄 우클릭 메뉴의 이전 아이콘 수정

창과 트레이 아이콘을 교체한 뒤에도 실행 파일에 포함된 Windows 아이콘 리소스는 이전 빌드 결과를 재사용하고 있었습니다. 실행 파일의 아이콘 이미지 6개를 현재 ICO의 이미지 6개와 비교했을 때 일치하는 이미지가 없었습니다.

`src-tauri/build.rs`에 `cargo:rerun-if-changed=icons`를 추가해 아이콘 폴더 변경 시 Tauri 빌드 스크립트와 실행 파일 리소스를 다시 생성하도록 수정했습니다. 작업표시줄 고정 및 우클릭 메뉴가 참조하는 실행 파일 아이콘에도 변경이 반영됩니다. Windows가 기존 고정 바로가기 아이콘을 캐시한 경우 고정을 해제한 뒤 새로 실행한 ACE를 다시 고정하세요.

수정 후 네이티브 빌드를 완료했고, 실행 파일 내부의 아이콘 이미지 6개가 현재 ACE ICO 이미지 6개와 모두 일치함을 확인했습니다.

## 다시 실행해도 우클릭 메뉴가 이전 아이콘인 경우

실행 파일 리소스 교체만으로 해결되지 않은 사용자 환경을 위해 `src-tauri/src/taskbar.rs`에서 창의 Windows Property Store에 다음 값을 직접 설정합니다.

- `System.AppUserModel.ID`: `com.ace.desktop`
- `System.AppUserModel.RelaunchCommand`: 현재 ACE 실행 파일 경로
- `System.AppUserModel.RelaunchDisplayNameResource`: `ACE`
- `System.AppUserModel.RelaunchIconResource`: ACE 전용 `ace-taskbar.ico,0`

`bundle.resources`에 ICO 파일을 등록해 개발 빌드와 배포에 포함합니다. 실행 파일 내부 아이콘 대신 별도 ACE 전용 경로를 지정하므로 이전 실행 파일 경로의 아이콘 캐시에 의존하지 않습니다. `SHChangeNotify(SHCNE_UPDATEITEM)`로 해당 실행 파일과 아이콘 변경을 알립니다. Explorer 종료나 전체 아이콘 캐시 삭제는 수행하지 않습니다.

근거: [Microsoft RelaunchIconResource](https://learn.microsoft.com/en-us/windows/win32/properties/props-system-appusermodel-relaunchiconresource), [SHChangeNotify](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shchangenotify).

추가 검증: `cargo test --offline --lib windows_taskbar_store_retains_explicit_ace_icon` 통과. 실제 Windows 창의 Property Store에 ACE 아이콘 경로·이름·AppUserModelID를 설정하고 다시 읽어 일치하는지 확인합니다. 문자열은 Windows 라이브러리의 소유권 관리가 적용된 `PROPVARIANT::from`으로 전달합니다. 아이콘 설정 실패는 로그로 남기며 앱 시작을 중단하지 않습니다.

실행 중인 ACE 창에서도 Property Store를 직접 읽어 앱 ID `com.ace.desktop`, 이름 `ACE`, 현재 실행 파일의 재실행 명령, 별도 `ace-taskbar.ico,0` 경로가 설정돼 있음을 확인했습니다. Shell에 전달하는 아이콘 경로는 Windows canonical 경로의 `\\?\` 접두사를 제거합니다.
