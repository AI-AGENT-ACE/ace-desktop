# ACE Native Tool Contract 1.1

현재 24개 외부 도구의 입력·결과 JSON 계약은 NestJS 카탈로그에서 내보낸 [`native-tool-contract.json`](./native-tool-contract.json)이 기준입니다. 기존 `native-tool-contract.schema.json`은 이전 후보 참조 형식의 예제이며 최신 전체 계약이 아닙니다.

AI는 파일 검색 의도(query/destination)를 보낼 수 있습니다. 데스크톱은 사용자가 원본 파일과 대상 위치를 확인한 뒤 resourceId 기반 인자로 변환합니다. 모니터 ID·창 ID·승인 ID는 Rust 내부에서 발급하며 AI가 임의로 발급할 수 없습니다. 캡처 방식·영역·실행 ID와 승인 IPC의 계약도 위 JSON의 internalIpcContracts에 포함됩니다.

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
