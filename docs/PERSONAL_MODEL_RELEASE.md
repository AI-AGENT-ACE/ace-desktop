# 2026-11-03 개인 모델 초기 배포 결정

사용자가 현재 사용하는 개인 호출어 모델을 먼저 배포하고 필요 시 공용 모델로 교체하도록 명시적으로 요청함.

- 모델 원본: 앱 설정 폴더의 wake-word/user-reference.rpw. 원본 녹음은 포함하지 않음.
- 배포 사본: resources/wake-word/ace.rpw. 사용자 개인 모델 파일은 변경하지 않음.
- releasePolicy: personal-preview. evaluation.json의 pending은 평가 미실시이며 통과를 의미하지 않음.
- evaluated-public 기본 정책의 기존 여러 사람·오탐 평가 기준 유지. personal-preview만 명시적인 대기 기록 허용.
- 두 정책 모두 파일 존재·크기·버전·CBOR 형식·특징 값·SHA-256·배포 동의 기록 검증 유지.
- 설치 리소스에서 기본 모델 로드, 사용자 설정 경로의 개인 모델과 분리. 기존 preferPersonal 선택 유지.
- 업데이트 서명 키는 .local/release-signing에 보관하고 Git에 포함하지 않음. 해당 키는 향후 업데이트에도 유지해야 함.
- 운영 백엔드 호환성 및 새 PC 설치 시험은 별도이며 이번 모델 구조 검증으로 대체하지 않음.
