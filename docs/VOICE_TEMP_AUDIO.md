# Voice Command Temporary Audio

ACE 음성 명령은 영구 녹음이나 채팅 첨부가 아니라 처리용 임시 데이터입니다.

- Rust 저장 위치: OS Temp의 `ace/voice`
- Desktop 반환값: 실제 경로 없이 `recordingId`, 상태, 길이와 크기
- 전송: Rust가 JWT를 사용해 `POST /voice/commands`로 multipart 업로드
- 재시도: 네트워크·서버 오류에 최대 3회
- 정상 정리: 처리 성공 또는 최종 실패 직후 로컬 WAV 삭제
- 비정상 종료 정리: 시작 시 기본 1시간 TTL 초과 `.wav`, `.wav.part` 삭제
- Production UI: 임시 경로 비노출

Desktop 환경변수:

```text
ACE_VOICE_TEMP_TTL_SECONDS=3600
ACE_VOICE_UPLOAD_TIMEOUT_SECONDS=15
ACE_VOICE_PROCESSING_TIMEOUT_SECONDS=60
ACE_VOICE_MAX_FILE_SIZE=20971520
```

Raw PCM, WAV Binary, JWT와 실제 임시 경로는 로그나 데이터베이스에 저장하지 않습니다.
