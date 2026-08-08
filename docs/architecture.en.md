# System Architecture

English | [简体中文](architecture.md)

## Goals and Constraints

The system must extract every body image while Tieba's HTML and anti-abuse responses evolve, keep request pressure controlled, protect credentials, and resume large jobs after interruption. The architecture therefore separates access and authentication, content discovery, planning, and reliable downloading, with explicit data models between stages.

## High-Level Architecture

```mermaid
flowchart TB
    CLI[CLI / Interactive Wizard] --> CFG[Config]
    CFG --> PRE[Single HTTP Preflight]
    PRE -->|Legacy HTML| HTML[HTML Parser]
    PRE -->|403 / Challenge / CSR| AUTH[Chrome Auth]
    AUTH --> CDP[CDP Network Capture]
    CDP --> API[page_pc JSON Parser]
    HTML --> PLAN[ImageRecord List]
    API --> PLAN
    PLAN --> DEDUP[Stable Sort and Deduplication]
    DEDUP --> MANIFEST[(manifest.json)]
    DEDUP --> SCHED[Adaptive Scheduler]
    SCHED --> DL[Streaming Resume Downloader]
    DL --> FILES[(Images / .part)]
    DL --> STATE[(download-state.json)]
    DL --> FAIL[(failed.json)]
    AUTH <--> KEYCHAIN[(macOS Keychain)]
```

The dual parser keeps compatibility with legacy pages without depending on a virtualized DOM. A client-rendered page only mounts visible items, so scrolling the DOM can miss images. Capturing successful `page_pc` responses provides complete pagination without copying or reverse-engineering signed requests.

## Runtime Sequence

```mermaid
sequenceDiagram
    actor U as User
    participant A as Application
    participant T as Tieba
    participant C as Isolated Chrome
    participant K as Keychain
    participant D as Image CDN

    A->>K: Load saved Baidu session
    A->>T: Single-request preflight
    alt Parseable HTML
        T-->>A: Thread HTML
    else Challenge, 403, or CSR
        A->>C: Launch dedicated profile and local CDP
        C->>T: Open official thread page
        opt Verification required
            U->>C: Log in / complete verification normally
        end
        C-->>A: baidu.com cookies + paginated API responses
        A->>K: Optionally save session
    end
    A->>A: Parse, sort, deduplicate, write manifest
    loop Each image batch
        A->>D: GET or Range GET
        D-->>A: image/* bytes
        A->>A: Write .part, atomically update state, rename on completion
    end
```

## Authentication State Machine

```mermaid
stateDiagram-v2
    [*] --> Preflight
    Preflight --> ParseHTML: 2xx server-rendered HTML
    Preflight --> Browser: challenge / 403 / CSR
    Browser --> Waiting: Chrome started
    Waiting --> Waiting: login or verification incomplete
    Waiting --> Capture: normal thread page
    Capture --> Ready: all page responses captured
    Capture --> Failed: timeout or CDP disconnect
    ParseHTML --> Ready
    Ready --> [*]
    Failed --> [*]
```

Chrome uses an application-owned profile and a random local debugging port. The program selects only `baidu.com` cookies and never opens the user's regular browser profile. CDP listens on `127.0.0.1`, and the dedicated browser is closed when capture finishes.

## Data Model and Boundaries

The central `ImageRecord` contains page, post order, image order, floor, author, post ID, source URL, normalized URL, and target filename. Parsers only produce records; deduplication only establishes order and names; the downloader only consumes records. A page-format change therefore stays out of resume logic, while a download-protocol change stays out of parsers.

State is updated atomically by writing, flushing, and renaming a temporary file. Images are written to same-directory `.part` files and renamed only after complete receipt, preventing a crash from presenting partial data as complete.

## Concurrency and Error Control

Page scanning and image downloading have independent limits. Downloads begin conservatively and grow after successful windows. HTTP 403/429 halves effective concurrency and honors `Retry-After` or the configured cooldown. DNS and connection failures use per-item exponential backoff without blocking completed work.

```mermaid
flowchart TD
    B[Start batch] --> R[Concurrent requests]
    R --> Q{Any 403/429?}
    Q -- Yes --> DOWN[Halve concurrency]
    DOWN --> COOL[Wait Retry-After / cooldown]
    COOL --> B
    Q -- No --> E{Transient error?}
    E -- Yes --> RETRY[Per-item backoff and requeue]
    RETRY --> B
    E -- No --> UP[Gradually increase after success]
    UP --> N{Tasks remain?}
    N -- Yes --> B
    N -- No --> DONE[Write final state]
```

## Security Boundary

- No CAPTCHA solving, request bypass construction, or access-control bypass.
- No personal browser database access; only the application-owned Chrome profile is used.
- Cookies never enter logs, manifests, download state, or failure reports.
- Downloads require successful status and a recognized `image/*` MIME type.
- URLs, cookie files, concurrency values, and Range responses are validated before use.
