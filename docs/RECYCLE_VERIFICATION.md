# 파일 휴지통 이동 오류 조사 (2026-10-16)

- Windows 실제 바탕화면: C:\Users\hwnsng\Desktop. ACE-Native-Test\삭제용.txt 존재, 26바이트, Archive 속성 확인. 원본 파일은 변경하지 않음.
- Native 파일 경로는 canonicalize()를 거쳐 verbatim drive 경로(\\?\C:\...)가 됨.
- 별도 생성 파일로 재현: 파일 존재 검사 통과, GetDriveTypeW=3(고정 드라이브), SHCreateItemFromParsingName에서 HRESULT 0x80070057(매개 변수가 틀립니다) 실패. 휴지통 지원 드라이브를 잘못 판단한 문제가 아님.
- Shell에 전달할 때만 verbatim drive prefix를 변환하고, 변환 전후 canonicalize 결과 동일성을 검사함. UNC/장치 경로와 끝에 점·공백이 있는 모호한 경로는 거부함.
- 재파싱 지점 거부, 휴지통 전용 플래그, PreDeleteItem의 recycle 확인 및 영구 삭제 거부를 유지함.
- 개발 빌드에서 metadata/drive/path/COM 생성/셸 항목 생성/삭제 등록/실행/콜백/최종 결과를 구분해 기록함. 오류에는 OS 코드 또는 HRESULT와 메시지 포함. 전체 사용자 파일 경로는 일반 작업 로그에 추가하지 않음.

## 검증

- 경로 변환·동일 대상 유지, UNC/장치 경로 거부, 비휴지통 삭제 거부 단위 테스트 3개 통과.
- 실제 바탕화면 테스트 파일 이동 1개 통과. PreDeleteItem flags=0x280, PostDeleteItem HRESULT=0x00270008, recycled_item=true, aborted=false 확인.
- Shell.Application의 사용자 휴지통 조회로 아래 파일 존재 확인. 원래 위치에서는 제거됨.
- 파일: ace-recycle-fixture-441c1749-1198-4ea4-9074-2a6640ad0f94.txt
- 원래 폴더: C:\Users\hwnsng\Desktop\ACE-Native-Test\ace-recycle-test-8b1d4436-61ef-4c88-97d6-cfe88410c373
- 실제 테스트는 공통 Rust recycle 함수를 호출함. 채팅/승인 UI 전체를 다시 실행한 시험과는 구분함.

## 수동 복원

1. Windows 휴지통을 열고 위 파일 이름으로 검색.
2. 원래 위치가 위 테스트 폴더인지 확인.
3. 해당 파일만 우클릭 → 복원.
4. 원래 폴더에서 파일을 열어 `ACE generated recycle verification fixture` 내용 확인.

복원 확인용 폴더와 휴지통의 테스트 파일은 남겨둠. 모든 항목 복원/휴지통 비우기는 필요 없음. 개발 앱을 재시작한 뒤 Native 패널의 기존 삭제용.txt도 사용자 승인으로 다시 시험 가능.
