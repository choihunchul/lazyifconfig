# Route Inspector 설계 문서

- **작성일**: 2026-06-11
- **상태**: 구현 계획 완료

## 개요

`Route Inspector`는 기존 Routes view를 단순 라우팅 테이블 화면에서 시각적 라우팅 진단 화면으로 승격하는 기능입니다. 목표는 사용자가 라우팅 테이블 문법을 몰라도 "내 트래픽이 어느 인터페이스와 게이트웨이를 통해 나가는지", "VPN이 라우팅에 어떤 영향을 주는지", "특정 목적지까지 어떤 경로가 선택되는지"를 이해하게 만드는 것입니다.

1차 구현은 현재 프로젝트의 지원 범위에 맞춰 macOS와 Linux를 대상으로 합니다. Windows 명령(`Get-NetRoute`, `Test-NetConnection`, `route print`)은 데이터 모델과 명령 추상화가 확장될 수 있도록 future work로 문서화하되, 이번 구현에는 포함하지 않습니다.

---

## 1. 목표와 범위

### 목표

1. **Default route를 즉시 이해할 수 있는 Summary 제공**
   - 기본 게이트웨이, 기본 인터페이스, IPv4/IPv6 route 수, VPN 상태, warning 수를 상단에 표시합니다.
2. **Path Viewer를 대표 기능으로 제공**
   - 사용자가 `8.8.8.8`, `1.1.1.1`, `github.com` 같은 destination을 입력하면 OS가 선택한 route를 조회합니다.
   - 결과를 host -> interface -> gateway -> optional VPN -> destination 형태의 terminal topology로 렌더링합니다.
3. **Route Table을 진단 가능한 테이블로 개선**
   - destination, gateway, interface, metric, protocol, flags, family를 표시합니다.
   - default route와 VPN route를 색상으로 강조합니다.
4. **VPN Routes와 Diagnostics를 별도 섹션으로 제공**
   - `tun*`, `tap*`, `utun*`, `wg*`, `tailscale*`, `zt*` 계열 인터페이스를 VPN 후보로 감지합니다.
   - default route 누락, 중복 default route, down interface 참조를 warning으로 표시하고, VPN default override는 의도된 VPN 사용일 수 있으므로 informational diagnostic으로 표시합니다.
5. **Raw Output과 연결**
   - 기존 Raw Output Viewer를 재사용하여 `netstat -rn`, `route -n get default`, `ip route show`, `ip -6 route show`, `ip rule`, route-get 결과를 확인할 수 있게 합니다.

### 비목표

1. 이번 구현에서 Windows를 지원하지 않습니다.
2. DNS, Nmap을 실제 top-level 기능으로 추가하지 않습니다. 사용자가 제시한 `[Interfaces] [DNS] [Routes] [Ports] [Nmap] [Raw]`는 장기 내비게이션 방향으로 남기고, 이번 구현은 현재 `ViewMode::Routes`를 Route Inspector로 확장합니다.
3. 실제 traceroute 실행, 네트워크 디스커버리, live traffic overlay, route history persistence는 이번 구현에 포함하지 않습니다.

---

## 2. 사용자 경험

### 내비게이션

기존 `g` 단축키는 유지합니다. Routes view에 들어오면 제목과 footer는 Route Inspector의 섹션 기반 흐름을 반영합니다.

```text
i interfaces | n networks | c connections | p ports | e timeline | g routes
```

Routes 화면 내부 섹션:

```text
Routes
├─ Summary
├─ Path Viewer
├─ Route Table
├─ VPN Routes
├─ Diagnostics
└─ Raw Output
```

초기 MVP에서는 섹션을 별도 top-level tabs로 만들기보다 한 화면에 압축 배치합니다. 작은 터미널에서도 동작해야 하므로, 높이가 부족하면 Summary와 selected details를 우선하고 Route Table은 스크롤 가능한 list로 폴백합니다.

### Summary

상단 Summary는 가장 중요한 라우팅 정보를 먼저 보여줍니다.

```text
Default Route
────────────────────────────
Gateway: 192.168.0.1
Interface: en0

IPv4 Routes: 24
IPv6 Routes: 18

VPN: Connected
VPN Interface: utun4

Warnings: 1
```

색상:

- Green: active/default route
- Blue: gateway
- Yellow: VPN
- Red: warning
- Gray: inactive or unresolved details

### Path Viewer

Path Viewer는 Route Inspector의 중심 기능입니다.

사용자는 route destination input을 활성화한 뒤 destination을 입력합니다.

```text
8.8.8.8
1.1.1.1
github.com
```

실행 명령:

- Linux: `ip route get <destination>`
- macOS: `route -n get <destination>`

결과는 `RoutePathResult`로 파싱한 뒤 box drawing 문자를 사용하는 terminal topology로 렌더링합니다.

일반 경로:

```text
┌──────────────┐
│ This Host    │
│192.168.0.25  │
└──────┬───────┘
       │ en0
       ▼
┌──────────────┐
│ Gateway      │
│192.168.0.1   │
└──────┬───────┘
       │
       ▼
┌──────────────┐
│ Internet     │
└──────┬───────┘
       │
       ▼
┌──────────────┐
│ 8.8.8.8      │
└──────────────┘
```

VPN 경로:

```text
┌──────────────┐
│ This Host    │
└──────┬───────┘
       │ en0
       ▼
┌──────────────┐
│ Gateway      │
└──────┬───────┘
       │
       ▼
┌──────────────┐
│ VPN Tunnel   │
│ utun4        │
└──────┬───────┘
       │
       ▼
┌──────────────┐
│ Destination  │
│ 8.8.8.8      │
└──────────────┘
```

도메인을 입력한 경우 OS 명령이 직접 처리하지 못하는 환경이 있을 수 있습니다. 1차 구현은 명령 실행 결과를 우선 사용하고, 실패 시 "destination could not be resolved by route command" warning을 표시합니다. 별도 DNS resolver 통합은 future work입니다.

---

## 3. 데이터 모델

### RouteEntry 확장

`src/model.rs`의 기존 `RouteEntry`를 확장합니다.

```rust
pub enum RouteFamily {
    Ipv4,
    Ipv6,
    Unknown,
}

pub struct RouteEntry {
    pub destination: String,
    pub gateway: String,
    pub interface: String,
    pub metric: Option<u32>,
    pub protocol: Option<String>,
    pub flags: Option<String>,
    pub family: RouteFamily,
}
```

기존 테스트와 navigation code는 destination/gateway/interface 중심으로 동작하므로, optional 필드를 통해 하위 호환성을 유지합니다.

### Path Viewer 모델

```rust
pub struct RoutePathQuery {
    pub destination: String,
}

pub struct RoutePathResult {
    pub destination: String,
    pub resolved_destination: Option<String>,
    pub source_ip: Option<String>,
    pub interface: Option<String>,
    pub gateway: Option<String>,
    pub is_vpn: bool,
    pub raw_output: String,
}

pub enum RouteGraphNodeKind {
    Host,
    Interface,
    Gateway,
    VpnTunnel,
    Internet,
    Destination,
}

pub struct RouteGraphNode {
    pub kind: RouteGraphNodeKind,
    pub label: String,
    pub detail: Option<String>,
}

pub struct RouteGraph {
    pub nodes: Vec<RouteGraphNode>,
}
```

`petgraph`는 graph builder 내부에서 사용할 수 있도록 도입하되, TUI 렌더러는 단순한 `RouteGraph` DTO를 받습니다. 이렇게 하면 이후 traceroute, path comparison, route history를 붙일 때 내부 graph 표현을 바꾸더라도 UI와 app state 영향이 작습니다.

### Diagnostics 모델

```rust
pub enum RouteDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

pub struct RouteDiagnostic {
    pub severity: RouteDiagnosticSeverity,
    pub title: String,
    pub description: String,
    pub affected_route: Option<RouteEntry>,
    pub recommendation: String,
}
```

---

## 4. 아키텍처

Route Inspector는 다음 파이프라인으로 구성합니다.

```text
RouteCollector
    ↓
RouteParser
    ↓
RouteGraphBuilder
    ↓
RouteDiagnostics
    ↓
TUI Renderer
```

### RouteCollector

`src/command.rs`에 OS별 command spec을 추가합니다.

- `route_table_command_spec()`: 기존 함수 유지
- `default_route_command_spec()`: 기존 함수 유지
- `route_path_command_spec(destination: &str)`: 신규
- `ipv6_route_table_command_spec()`: Linux에서 `ip -6 route show`; macOS는 `netstat -rn` 출력에 IPv6가 포함되므로 별도 명령 없이 raw source mapping만 조정
- `ip_rule_command_spec()`: Linux 전용 `ip rule`

Route path 명령 결과는 기존 `CommandOutput` 저장소에도 기록합니다. destination별 raw output은 `CommandSourceId`가 동적으로 늘어나기 어렵기 때문에, 1차 구현에서는 `CommandSourceId::RoutePath`를 추가하고 최신 route-get 결과를 저장합니다.

### RouteParser

`src/collector/routes.rs`를 다음 역할로 확장합니다.

- macOS `netstat -rn` IPv4/IPv6 table parse
- Linux `ip route show` parse
- Linux `ip -6 route show` parse
- macOS `route -n get <destination>` parse
- Linux `ip route get <destination>` parse

파서 정책:

1. 알 수 없는 컬럼은 버리지 않고 가능한 경우 `flags` 또는 `protocol`에 보존합니다.
2. metric이 없는 OS 출력은 `None`으로 둡니다.
3. 파싱 실패는 panic 대신 빈 result 또는 diagnostic으로 전환 가능한 error를 반환합니다.

### RouteGraphBuilder

`RoutePathResult`와 현재 `NetworkSnapshot`을 받아 `RouteGraph`를 생성합니다.

규칙:

1. source IP가 있으면 Host node detail로 표시합니다.
2. interface가 있으면 Interface node를 추가합니다.
3. gateway가 있고 `link`, `link#*`, `default`, `local`이 아니면 Gateway node를 추가합니다.
4. interface가 VPN 패턴과 일치하거나 default route가 VPN interface를 가리키면 VPN Tunnel node를 추가합니다.
5. 마지막은 Destination node입니다.

### RouteDiagnostics

초기 rule set:

1. **No default route**
   - default destination이 하나도 없으면 warning.
2. **Multiple default routes**
   - IPv4 default route가 2개 이상이면 warning.
3. **Interface referenced by route is down**
   - route interface가 snapshot interface에 존재하고 status가 Down이면 warning.
4. **Route references missing interface**
   - route interface가 snapshot에 없으면 warning.
5. **VPN overrides default route**
   - default route interface가 VPN 패턴과 일치하면 Info로 표시합니다. VPN 연결 의도일 수 있으므로 warning count에는 포함하지 않습니다.
6. **Route metric conflict**
   - 같은 destination에 metric이 같은 route가 2개 이상이면 warning.
7. **Gateway unreachable**
   - 1차 구현에서는 강한 판정을 하지 않습니다. gateway가 비어 있거나 parse되지 않은 경우 Info로 표시하고, 실제 reachability check는 future work로 남깁니다.

---

## 5. App State와 키 흐름

`src/app.rs`에 `RouteInspectorState`를 추가합니다.

```rust
pub enum RouteInspectorSection {
    Summary,
    PathViewer,
    RouteTable,
    VpnRoutes,
    Diagnostics,
}

pub struct RouteInspectorState {
    pub active_section: RouteInspectorSection,
    pub destination_input: String,
    pub destination_input_active: bool,
    pub latest_path_result: Option<RoutePathResult>,
    pub latest_path_error: Option<String>,
    pub diagnostics: Vec<RouteDiagnostic>,
    pub route_filter: String,
    pub route_filter_active: bool,
    pub sort_column: RouteSortColumn,
}
```

키 흐름:

- `g`: Route Inspector 진입
- `Tab` / `Shift+Tab`: Route Inspector 내부 섹션 전환
- `/`: Route Table filter 활성화
- `Enter`: Path Viewer destination 입력 실행 또는 filter 적용
- `Esc`: 입력 모드 해제
- `o`: 기존 Raw Output Viewer 열기
- `j/k`: 현재 섹션 또는 route list 이동

기존 `raw_viewer`가 활성화된 경우에는 기존 raw viewer 키 핸들러가 우선합니다.

---

## 6. UI 렌더링

기존 `ui.rs`의 `ViewMode::Routes` branch를 Route Inspector 렌더러로 분리합니다.

추천 함수:

- `render_route_inspector(frame, app, area)`
- `render_route_summary(app) -> Vec<Line>`
- `render_route_table(app) -> List`
- `render_path_viewer(app) -> Paragraph`
- `render_vpn_routes(app) -> Paragraph`
- `render_route_diagnostics(app) -> Paragraph`
- `render_route_graph(graph: &RouteGraph) -> Vec<Line>`

작은 화면 폴백:

1. 폭이 좁으면 ASCII box 폭을 줄이고 detail은 다음 줄로 내립니다.
2. 높이가 부족하면 Summary와 selected section만 렌더링합니다.
3. Path Viewer 결과가 없으면 현재 default route 기반 preview를 보여줍니다.

---

## 7. Raw Output 연동

Routes view에서 Raw Output Viewer가 순회할 source:

macOS:

- `netstat -rn`
- `route -n get default`
- latest `route -n get <destination>` if available
- public IP curl output if already captured

Linux:

- `ip route show`
- `ip -6 route show`
- `ip rule`
- latest `ip route get <destination>` if available
- public IP curl output if already captured

Raw Output Viewer 요구 사항은 기존 구현을 재사용합니다.

- monospace terminal style
- scroll
- search
- copy
- source switching

---

## 8. 테스트 계획

### Unit tests

1. `parse_routes`가 기존 macOS IPv4 route를 계속 파싱하는지 확인합니다.
2. macOS `netstat -rn`의 IPv6 section을 `RouteFamily::Ipv6`로 파싱합니다.
3. Linux `ip route show`에서 metric/proto/scope/src를 가능한 범위로 파싱합니다.
4. Linux `ip -6 route show`를 IPv6 route로 파싱합니다.
5. macOS `route -n get 8.8.8.8` 결과를 `RoutePathResult`로 파싱합니다.
6. Linux `ip route get 8.8.8.8` 결과를 `RoutePathResult`로 파싱합니다.
7. VPN interface detector가 `tun0`, `tap0`, `utun4`, `wg0`, `tailscale0`, `ztabc`를 감지합니다.
8. diagnostics가 no default route, multiple default routes, route interface down, missing interface를 감지합니다.
9. `RouteGraphBuilder`가 일반 route와 VPN route의 node sequence를 올바르게 생성합니다.

### App state tests

1. `ViewMode::Routes` 진입 시 route navigation이 기존처럼 동작합니다.
2. route filter가 destination/gateway/interface에 적용됩니다.
3. Route Inspector section 전환이 selection과 scroll을 깨뜨리지 않습니다.
4. route path result 저장 후 Raw Output source 목록에 latest path output이 포함됩니다.

### Manual verification

1. macOS에서 `cargo run` 후 `g`로 Route Inspector에 진입합니다.
2. `8.8.8.8`, `1.1.1.1`, `github.com` destination을 입력해 Path Viewer가 갱신되는지 확인합니다.
3. VPN 연결 상태에서 default route와 VPN Routes panel이 바뀌는지 확인합니다.
4. 작은 터미널 크기에서 Summary와 Path Viewer가 깨지지 않는지 확인합니다.
5. `o`로 Raw Output Viewer를 열어 route 관련 raw source를 검색/복사할 수 있는지 확인합니다.

---

## 9. Future Work

1. Windows support: `route print`, `Get-NetRoute`, `Test-NetConnection` parser 추가.
2. DNS resolver integration: domain destination을 명시적으로 resolve하고 resolved IP를 path graph에 표시.
3. Traceroute visualization: route decision 이후 실제 hop sequence를 graph에 결합.
4. Network discovery/Nmap integration: local subnet discovery 결과를 topology graph에 표시.
5. Live traffic overlays: interface stats history를 path edge 위에 표시.
6. Path comparison: VPN on/off 또는 destination 간 route 차이 비교.
7. Route change history: default route와 VPN override 변화를 timeline에 더 세밀하게 기록.
