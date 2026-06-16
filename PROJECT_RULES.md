# Project Rules

- 기본 규칙: 본 프로젝트에서 작업은 `caveman` SKILL을 기본 적용한다.
- 릴리스 관련 GitHub Action 명시
  - `/.github/workflows/create-release-tag.yml`: `v*` 태그 생성을 수동으로 수행하는 워크플로우.
  - `/.github/workflows/release.yml`: 태그(`v*`) 푸시 또는 수동 입력(`tag`) 시 멀티 아키텍처 릴리스 아티팩트를 빌드·패키징·업로드한다.
  - `/.github/workflows/publish-crate.yml`: 수동으로 `crates.io` 릴리스를 검증/발행한다(선택 옵션: `dry_run_only`).
  - `/.github/workflows/publish-homebrew-tap.yml`: 수동으로 Homebrew Tap 업로드를 생성·갱신한다.
- 체크포인트마다 커밋
  - 기능/문서/릴리스 준비 단계를 작은 체크포인트로 나누고 각 체크포인트 완료 시점마다 `git commit`을 수행한다.
  - 예: `feat`, `chore`, `docs` 접두사를 사용해 변경 범위를 명확히 남긴다.
