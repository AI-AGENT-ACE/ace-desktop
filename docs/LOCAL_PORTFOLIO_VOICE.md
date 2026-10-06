# 로컬 한국어 음성 명령으로 포트폴리오 열기

AI 서버 연결 전 사용할 임시 로컬 기능이다. 음성의 실제 내용을 Whisper로 인식하고, 지원하는 포트폴리오 열기 문장과 전체 문장이 일치할 때만 `바탕화면/김환성_포트폴리오.pdf`를 연다. 아무 말이나 하면 여는 데모는 사용하지 않는다.

## 현재 PC 설정

- `node scripts/setup-local-stt.mjs` 설치 완료.
- `.env.local`에 `VITE_VOICE_MODE=local-portfolio` 적용.
- 대상 PDF의 존재 확인.
- whisper.cpp v1.9.2 Windows x64 CPU 런타임과 multilingual small-q5_1 모델(약 190 MB)을 `.local/whisper`에 저장. 공식 배포 파일과 모델 SHA-256을 확인한다. Git에서 제외된다.
- Windows 기본 인식 엔진은 en-US만 등록되어 있어 한국어 명령 처리에 사용하지 않았다.

ACE를 재시작한 뒤 “ACE” → “바탕화면에서 포트폴리오 열어줘” → 약 1.5초 무음을 유지한다. 로컬 인식에 수 초 걸릴 수 있다. 지원 문장을 인식한 경우 기존 `submit_voice_tool_call` → 메인 창의 `file.open` → Rust 경로 검증 → 기본 PDF 뷰어로 열린다.

지원 예: “포트폴리오 열어줘”, “바탕화면에서 포트폴리오 열어줘”, “바탕화면에 있는 김환성 포트폴리오 PDF 열어 주세요”. 공백과 일반 문장 부호는 허용하지만 “열지 마”, 삭제 요청, 다른 파일 요청, 추가 문장이 붙은 요청은 실행하지 않는다. 명령 불일치는 Orb에 실제 인식 문장을 보여주며 다시 말하도록 안내한다. 음성 인식의 오인식 가능성은 남아 있다.

## 처리와 임시 파일

VoiceWindow가 WAV 확정 후 `transcribe_local_portfolio(recordingId)`를 호출한다. Rust는 메모리 등록표에서 파일을 찾아 단일 처리권을 가져온다. UI에서 임의 파일 경로나 프로그램 인자를 받지 않는다. WAV를 mono 16kHz PCM으로 변환하고 고정 인자의 Whisper 실행 파일을 숨김 창으로 실행한다. 한국어, CPU, temperature fallback 없음으로 처리하며 전체 문장 제한 검사는 Rust에서 수행한다.

처리 시간 제한은 90초이며 시간 초과 시 자식 프로세스를 종료한다. 원본 임시 녹음, 변환 WAV 및 인식 JSON은 정상·오류 종료 시 정리한다. 음성·인식 원문을 터미널 로그에 출력하거나 백엔드/외부 API에 업로드하지 않는다. 창을 닫은 뒤 돌아온 결과로 파일을 실행하지 않도록 현재 음성 세대를 확인한다.

## 재설치와 AI 경로 복원

새 clone에서는 아래를 실행하고 `.env.local`에 모드를 설정한다.

```powershell
node scripts/setup-local-stt.mjs
# .env.local: VITE_VOICE_MODE=local-portfolio
npm run tauri dev
```

다른 설치 경로를 사용할 때 Rust 프로세스의 `ACE_LOCAL_STT_DIR`에 런타임과 모델이 들어 있는 폴더를 지정할 수 있다. 기본은 소스 저장소의 `.local/whisper`다. 설치형 앱에 모델을 자동 배포하는 구성은 이번 범위에 포함하지 않았다.

추후 `.env.local`의 `VITE_VOICE_MODE`를 제거하고 Vite/Tauri를 재시작하면 기존 백엔드 음성 API 경로로 돌아간다.

## 확인한 결과

- 실제 기존 일반 발화 30초 WAV를 로컬 엔진으로 변환: 99자 인식, 포트폴리오 요청 false, 약 6.9초. 원문은 로그에 출력하지 않았다.
- Rust: 허용 명령/부정·다른 요청 거절과 오디오 변환 길이·진폭 검사 통과.
- 프론트 회귀 검사: 지정 문장의 정확한 파일 매핑, 녹음 종료, 인증 전달 등 4개 통과.
- TypeScript/Vite 빌드 통과. 실제 사용자 목소리의 포트폴리오 문장부터 PDF 창 표시까지는 사용자 마이크로 재확인이 필요하다.
- 엄격 Clippy 검사 통과. 비정상 종료로 남는 로컬 인식 JSON도 기존 임시 음성 TTL 정리에 포함한다.
- 한국어 합성 음성으로 양성 샘플 생성을 시도했지만 Windows TTS 실행 오류로 실패했다. 이를 실제 음성 인식 성공으로 기록하지 않았다.

공식 소스: https://github.com/ggml-org/whisper.cpp

공식 모델 다운로드 경로: https://github.com/ggml-org/whisper.cpp/blob/master/models/download-ggml-model.sh
