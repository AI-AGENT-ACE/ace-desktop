export const toolGroups = [
  '앱',
  '파일',
  '폴더',
  '인터넷',
  '클립보드',
  '음성',
  '화면',
  '시스템',
] as const;
type Presentation = { group: (typeof toolGroups)[number]; name: string; description: string };
export const toolPresentations: Record<string, Presentation> = {
  'app.open': { group: '앱', name: '앱 실행', description: '선택한 앱을 실행합니다.' },
  'app.close': {
    group: '앱',
    name: '앱 창 닫기',
    description: '선택한 창에 종료를 요청합니다. 저장 확인이 나오면 직접 처리할 수 있습니다.',
  },
  'app.focus': {
    group: '앱',
    name: '앱 창 앞으로 가져오기',
    description: '선택한 창을 화면 앞으로 가져옵니다. 최소화된 창은 복원합니다.',
  },
  'app.minimize': {
    group: '앱',
    name: '앱 창 최소화',
    description: '선택한 창을 작업 표시줄로 내립니다.',
  },
  'app.maximize': {
    group: '앱',
    name: '앱 창 최대화',
    description: '선택한 창을 화면에 크게 표시합니다.',
  },
  'file.search': {
    group: '파일',
    name: '파일 찾기',
    description: '접근이 허용된 폴더에서 파일을 검색합니다.',
  },
  'file.open': {
    group: '파일',
    name: '파일 열기',
    description: '선택한 파일을 연결된 프로그램으로 엽니다.',
  },
  'file.rename': {
    group: '파일',
    name: '파일 이름 바꾸기',
    description: '선택한 파일의 이름을 변경합니다.',
  },
  'file.move': {
    group: '파일',
    name: '파일 이동',
    description: '파일을 다른 위치로 옮깁니다. 완료되면 원래 위치에는 남지 않습니다.',
  },
  'file.copy': {
    group: '파일',
    name: '파일 복사',
    description: '원본을 유지하고 다른 위치나 이름으로 사본을 만듭니다.',
  },
  'file.delete': {
    group: '파일',
    name: '파일을 휴지통으로 이동',
    description: '선택한 파일을 휴지통으로 보냅니다. 영구 삭제하지 않습니다.',
  },
  'folder.open': {
    group: '폴더',
    name: '폴더 열기',
    description: '선택한 폴더를 파일 탐색기로 엽니다.',
  },
  'folder.search': {
    group: '폴더',
    name: '폴더 찾기',
    description: '접근이 허용된 범위에서 폴더를 검색합니다.',
  },
  'folder.create': {
    group: '폴더',
    name: '새 폴더 만들기',
    description: '지정한 위치에 폴더를 만듭니다.',
  },
  'web.open': {
    group: '인터넷',
    name: '웹사이트 열기',
    description: '기본 브라우저로 지정한 웹사이트를 엽니다.',
  },
  'web.search': {
    group: '인터넷',
    name: '인터넷 검색',
    description: '기본 브라우저에서 입력한 검색어로 검색합니다.',
  },
  'clipboard.write': {
    group: '클립보드',
    name: '클립보드에 텍스트 복사',
    description:
      '텍스트를 복사해 다른 곳에 붙여넣을 수 있게 합니다. 기존 클립보드 내용은 바뀝니다.',
  },
  'clipboard.read': {
    group: '클립보드',
    name: '복사한 텍스트 읽기',
    description: '클립보드의 텍스트를 읽습니다. 채팅 요청 시 읽은 내용을 AI에 전달합니다.',
  },
  'voice.activate': {
    group: '음성',
    name: '음성 입력 시작',
    description: '음성 입력 창을 열고 설정된 마이크로 녹음을 시작합니다.',
  },
  'screen.capture': {
    group: '화면',
    name: '화면 캡처',
    description: '선택한 모니터 전체 또는 지정한 영역을 이미지로 캡처합니다.',
  },
  'system.volume.set': {
    group: '시스템',
    name: '소리 크기 조절',
    description: '컴퓨터의 출력 음량을 조절합니다.',
  },
  'system.volume.mute': {
    group: '시스템',
    name: '음소거 설정·해제',
    description: '컴퓨터의 출력 소리를 끄거나 다시 켭니다.',
  },
  'system.open_settings': {
    group: '시스템',
    name: 'Windows 설정 열기',
    description: '요청한 Windows 설정 화면을 엽니다. 설정값을 직접 변경하지 않습니다.',
  },
  'system.lock': {
    group: '시스템',
    name: '컴퓨터 잠금',
    description: 'Windows 잠금 화면으로 전환합니다. 다시 사용하려면 잠금을 해제해야 합니다.',
  },
};
export function toolPresentation(tool: string): Presentation {
  return (
    toolPresentations[tool] || {
      group: '시스템',
      name: '추가 기능',
      description: '이 기능의 실행 권한을 설정합니다.',
    }
  );
}
