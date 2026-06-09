# Terminal-Themed Raw Output Viewer 설계 문서

- **작성일**: 2026-06-09
- **상태**: 승인 완료

## 개요

`lazyifconfig`는 시스템 네트워킹 명령을 실행하고 그 결과를 파싱하여 제공하는 TUI 도구입니다. 본 기능은 원본 명령 실행 결과의 투명성을 높이고 디버깅을 용이하게 하기 위해, 실제 터미널 세션을 사용하는 듯한 감각의 **터미널 테마 원시 출력 뷰어(Raw Output Viewer)**를 모달 오버레이 형식으로 추가합니다.

---

## 1. 요구 사항 및 핵심 원칙

1. **터미널 스타일 모달 오버레이**:
   - 현재 활성화된 화면의 컨텍스트를 유지하되, Pure Black 배경과 터미널 그린 텍스트의 모달창을 띄워 독립된 터미널 윈도우처럼 표현합니다.
   - 화면 해상도가 80x24 미만인 경우 모달 대신 전체 화면을 채우는 폴백(Fallback) 레이아웃을 사용합니다.
2. **실행 정보 저장**:
   - 각 명령 실행 시, 실제 실행한 커맨드 라인, 실행 시각(`SystemTime`), stdout/stderr, 종료 코드를 공통 저장소에 기록합니다.
3. **인터랙티브 기능**:
   - **소스 전환**: `Tab` / `Shift+Tab`을 이용해 현재 뷰가 의존하는 여러 명령의 원시 결과 사이를 손쉽게 전환할 수 있습니다.
   - **텍스트 검색**: `/` 키로 검색 모드로 진입하여 키 입력을 가로채고, 매칭되는 텍스트를 황색 배경으로 하이라이팅 처리합니다. `n`/`N` 단축키로 이전/다음 검색 결과를 순회합니다.
   - **클립보드 복사**: `y`로 현재 명령어 텍스트, `Y`로 결합된 전체 원시 출력을 클립보드로 복사합니다.

---

## 2. 세부 설계 및 데이터 모델

### A. 명령어 매핑 규정
각 뷰 모드에서 원시 출력 뷰어 활성화 시 노출할 명령 소스는 다음과 같이 고정 매핑됩니다.

- **Interface View**: `ifconfig`
- **Network View**: `ifconfig`
- **Connections View**: `netstat -an`
- **Ports View**: `lsof -iTCP -sTCP:LISTEN -P -n`
- **Routes View**: `netstat -rn`, `route -n get default`
- **Timeline View**: `ifconfig`, `netstat -rn`, `route -n get default`
- **Public IP**: `curl -s -m 5 https://ipinfo.io/json` (Routes View 화면에서 `o`를 누르면 `curl`도 소스 리스트에 포함되어 순회 가능)

### B. 데이터 모델 구조 (`src/model.rs`)

명령 결과와 소스 ID를 열거형 및 구조체로 추가 정의합니다.

```rust
// src/model.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandSourceId {
    Ifconfig,
    NetstatRoutes,
    DefaultRoute,
    NetstatConnections,
    LsofPorts,
    PublicIp,
    Arp,
}

impl CommandSourceId {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommandSourceId::Ifconfig => "ifconfig",
            CommandSourceId::NetstatRoutes => "netstat -rn",
            CommandSourceId::DefaultRoute => "route -n get default",
            CommandSourceId::NetstatConnections => "netstat -an",
            CommandSourceId::LsofPorts => "lsof -iTCP -sTCP:LISTEN -P -n",
            CommandSourceId::PublicIp => "curl -s -m 5 https://ipinfo.io/json",
            CommandSourceId::Arp => "arp -a",
        }
    }
}

#[derive(Clone, Debug)]
pub struct CommandOutput {
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub executed_at: std::time::SystemTime,
    pub exit_code: Option<i32>,
}
```

### C. 뷰어 상태 캡슐화 (`src/app.rs`)

`App` 구조체 내에 독립된 `RawViewerState` 구조체를 정의하여 상태 필드들을 응집력 있게 묶어 관리합니다.

```rust
// src/app.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchMatch {
    pub line_index: usize,
    pub start_byte: usize,
    pub end_byte: usize,
}

#[derive(Clone, Debug, Default)]
pub struct RawViewerState {
    pub active: bool,
    pub sources: Vec<CommandSourceId>,
    pub selected_index: usize,
    pub scroll: u16,
    pub search_query: String,
    pub search_active: bool,
    pub search_matches: Vec<SearchMatch>,
    pub current_match_index: usize,
}

// App 구조체에 필드 추가
pub struct App {
    ...
    pub command_outputs: std::collections::HashMap<CommandSourceId, CommandOutput>,
    pub raw_viewer: RawViewerState,
}
```

---

## 3. 키 이벤트 흐름 및 모달 제어 로직 (`src/main.rs`)

뷰어가 활성화(`raw_viewer.active == true`)되면, 일반 화면의 키 조작 대신 뷰어 독점 키 핸들러로 분기합니다.

```text
[키 입력 분기]
- raw_viewer.search_active == true (검색 입력 중):
  * Esc: 검색 모드 해제 (검색어 유지)
  * Backspace: 검색어 문자열 pop 및 검색 매치 갱신
  * Enter: 검색 입력 완료 (검색 모드 비활성화 및 첫 번째 매치로 스크롤 점프)
  * Char(c): 검색어 문자열 push 및 검색 매치 갱신

- raw_viewer.search_active == false (일반 뷰어 탐색 중):
  * Esc / q / o: 뷰어 종료 (active = false)
  * Tab / Shift+Tab (Backtab): 소스 순회 전환 (scroll = 0 초기화 및 매치 재계산)
  * j / Down: 아래로 1행 스크롤
  * k / Up: 위로 1행 스크롤
  * PageUp / PageDown: 15행 단위 스크롤
  * Home / End: 맨 위 / 맨 아래 스크롤
  * /: 검색 입력 모드 전환 (search_active = true)
  * n: 다음 검색 매치 인덱스로 점프 및 해당 라인으로 스크롤 이동
  * N: 이전 검색 매치 인덱스로 점프 및 해당 라인으로 스크롤 이동
  * y: 클립보드에 명령어 복사
  * Y: 클립보드에 결합된 전체 원시 출력(stdout + stderr) 복사
```

---

## 4. UI 및 렌더링 스타일 상세 (`src/ui.rs`)

### A. 색상표
- **배경**: `Color::Rgb(0, 0, 0)` (Pure Black)
- **테두리선**: `Color::Rgb(68, 68, 68)`
- **본문 텍스트**: `Color::Rgb(192, 255, 192)` (Soft Terminal Green)
- **프롬프트 기호 ($)** 및 **명령어**: `Color::Rgb(0, 255, 102)` (Bright Green, Bold)
- **메타 데이터(시각, 종료 코드)**: `Color::Rgb(128, 128, 128)` (Gray)
- **검색 매치 하이라이팅**: `Color::Black` 글자색 + `Color::Rgb(255, 204, 0)` (Yellow) 배경색

### B. 텍스트 하이라이트 분할 처리
출력 텍스트를 슬라이싱하여 `Span` 배열을 빌드할 때, 바이트 경계 유효성을 검증하여 패닉을 원천 봉쇄합니다.

```rust
fn build_matched_line<'a>(line: &'a str, matches_in_line: &[SearchMatch], text_color: Color, highlight_color: Color) -> Line<'a> {
    let mut spans = Vec::new();
    let mut last_idx = 0;

    for m in matches_in_line {
        if line.is_char_boundary(m.start_byte) && line.is_char_boundary(m.end_byte) {
            if m.start_byte > last_idx && line.is_char_boundary(last_idx) {
                spans.push(Span::styled(&line[last_idx..m.start_byte], Style::default().fg(text_color)));
            }
            spans.push(Span::styled(
                &line[m.start_byte..m.end_byte],
                Style::default().fg(Color::Black).bg(highlight_color).add_modifier(Modifier::BOLD)
            ));
            last_idx = m.end_byte;
        }
    }

    if last_idx < line.len() && line.is_char_boundary(last_idx) {
        spans.push(Span::styled(&line[last_idx..], Style::default().fg(text_color)));
    }

    Line::from(spans)
}
```

---

## 5. 검증 계획

1. **단위 테스트 (`cargo test`)**:
   - `update_raw_viewer_search_matches`가 다중 매칭(한 라인에 여러 개 매치 등) 및 대소문자 구분 없이 정확히 검색 영역을 식별하는지 검증하는 단위 테스트 구현.
   - `build_matched_line`이 한글/유니코드가 포함된 비ASCII 원시 출력을 끊어 읽을 때에도 안전하게 바이트 경계를 유지하는지 확인.
2. **통합 테스트**:
   - 모달 활성화 후 `Tab` 키를 누르면 다음 소스로의 스위칭 상태(`selected_index`) 및 스크롤 리셋이 올바르게 일어나는지 검증.
   - 클립보드 복사 로직에 따른 최근 이벤트 timeline 발행 성공 여부 확인.
3. **수동 검증**:
   - 실행 화면에서 `o`를 입력하여 모달을 활성화한 후, `/`를 쳐서 텍스트를 검색해보고 매치 수(`current/total`)가 올바르게 하이라이트와 연동되는지 체크.
   - 터미널 크기를 강제로 줄여 모달이 안정적으로 전체 화면 크기로 변형되어 깨지지 않고 렌더링되는지 확인.
