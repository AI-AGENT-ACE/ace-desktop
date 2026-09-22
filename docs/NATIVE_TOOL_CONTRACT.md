# ACE Native Tool Contract 1.1

AI 서버와 ACE Desktop Rust Runtime은 [`native-tool-contract.schema.json`](./native-tool-contract.schema.json)을 Native Tool 계약의 단일 기준으로 사용합니다.

이름 기반 대상은 `canonicalId`, 사용자 원문인 `original`, 최대 5개의 `candidates`로 전달합니다. 실제 실행 파일 경로와 Windows 절대 경로는 AI가 만들지 않습니다. Rust가 Installed App Registry, Windows Known Folder와 파일 시스템을 조회해 최종 대상을 결정합니다.

파일과 폴더 검색 결과는 실제 경로 대신 10분 동안 유효한 `resourceId`와 `displayName`을 반환합니다. `file.open`, `file.rename`, `file.move`, `file.copy`, `file.delete`는 이 ID를 우선 사용합니다. 만료되거나 존재하지 않는 ID는 각각 `RESOURCE_EXPIRED`, `RESOURCE_NOT_FOUND`로 거절합니다.

기존 `appId`, 문자열 `directory/query`, `directory + path` 입력은 전환 기간에만 Rust 호환 계층에서 받습니다. 새 AI 요청과 프론트엔드 코드는 1.1 계약을 사용해야 하며 호환 입력은 다음 계약 정리 단계에서 제거합니다.

```json
{
  "version": "1.1",
  "type": "tool_call",
  "tool": "app.open",
  "arguments": {
    "canonicalId": "vscode",
    "original": "브이에스코드",
    "candidates": ["브이에스코드", "Visual Studio Code", "VS Code", "vscode"]
  }
}
```

```json
{
  "version": "1.1",
  "type": "tool_call",
  "tool": "file.search",
  "arguments": {
    "directory": {
      "canonicalId": "desktop",
      "original": "바탕화면",
      "candidates": ["바탕화면", "Desktop", "desktop"]
    },
    "query": {
      "original": "포폴",
      "candidates": ["포폴", "포트폴리오", "portfolio"]
    },
    "extensions": ["pdf"],
    "limit": 20
  }
}
```
