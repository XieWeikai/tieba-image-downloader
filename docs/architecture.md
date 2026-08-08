# 系统架构

[English](architecture.en.md) | 简体中文

## 目标与约束

系统需要在贴吧 HTML 结构和风控响应随时变化的前提下，完整提取正文原图，同时控制请求压力、保护登录凭据，并允许大批量任务在网络中断后恢复。因此架构分为“访问与认证”“内容发现”“计划生成”“可靠下载”四个阶段，每个阶段通过明确的数据结构交接。

## 总体架构

```mermaid
flowchart TB
    CLI[CLI / 交互向导] --> CFG[Config]
    CFG --> PRE[HTTP 单请求预检]
    PRE -->|传统 HTML| HTML[HTML Parser]
    PRE -->|403 / 验证 / CSR| AUTH[Chrome Auth]
    AUTH --> CDP[CDP Network Capture]
    CDP --> API[page_pc JSON Parser]
    HTML --> PLAN[ImageRecord 列表]
    API --> PLAN
    PLAN --> DEDUP[稳定排序与去重]
    DEDUP --> MANIFEST[(manifest.json)]
    DEDUP --> SCHED[自适应调度器]
    SCHED --> DL[流式断点下载器]
    DL --> FILES[(图片 / .part)]
    DL --> STATE[(download-state.json)]
    DL --> FAIL[(failed.json)]
    AUTH <--> KEYCHAIN[(macOS Keychain)]
```

这种双解析路径保留了对旧页面的兼容性，又不依赖虚拟列表 DOM。客户端页面只渲染当前视口，直接滚动 DOM 容易漏图；捕获浏览器已经成功发出的 `page_pc` 响应可以获得完整分页数据，同时无需复制或逆向请求签名。

## 运行时序

```mermaid
sequenceDiagram
    actor U as 用户
    participant A as 应用
    participant T as 贴吧
    participant C as 隔离 Chrome
    participant K as Keychain
    participant D as 图片 CDN

    A->>K: 读取已保存百度会话
    A->>T: 单请求预检
    alt HTML 可解析
        T-->>A: 帖子 HTML
    else 验证、403 或 CSR
        A->>C: 启动独立配置与本地 CDP
        C->>T: 打开官方帖子页面
        opt 百度要求验证
            U->>C: 正常登录/完成验证
        end
        C-->>A: baidu.com Cookie + 分页 API 响应
        A->>K: 可选保存会话
    end
    A->>A: 解析、排序、去重、生成 manifest
    loop 每个图片批次
        A->>D: GET 或 Range GET
        D-->>A: image/* 数据
        A->>A: 写 .part、原子更新状态、完成后重命名
    end
```

## 认证状态机

```mermaid
stateDiagram-v2
    [*] --> Preflight
    Preflight --> ParseHTML: 2xx 且服务端 HTML
    Preflight --> Browser: 安全验证 / 403 / CSR
    Browser --> Waiting: Chrome 已启动
    Waiting --> Waiting: 登录或验证尚未完成
    Waiting --> Capture: 正常帖子页面
    Capture --> Ready: 所有分页响应已捕获
    Capture --> Failed: 超时或 CDP 断开
    ParseHTML --> Ready
    Ready --> [*]
    Failed --> [*]
```

Chrome 使用应用专属 profile 和随机本地调试端口。程序只选择 `baidu.com` 域 Cookie，不接触用户日常浏览器的配置文件。CDP 连接仅监听 `127.0.0.1`，任务结束后关闭专用浏览器。

## 数据模型与边界

核心中间模型 `ImageRecord` 包含页码、帖子顺序、图片顺序、楼层、作者、帖子 ID、原始 URL、规范 URL 和目标文件名。解析器只负责产生记录；去重器只负责确定顺序与名称；下载器只消费记录。这样页面格式变化不会侵入续传逻辑，下载协议变化也不会影响解析器。

状态文件通过“写临时文件、刷新、重命名”原子更新。图片先写同目录 `.part`，完全接收后再重命名，避免崩溃后把半文件误认为成功结果。

## 并发与错误控制

页面扫描与图片下载有独立上限。下载从较小并发开始，成功窗口逐步增加；HTTP 403/429 会使有效并发减半，并遵守 `Retry-After` 或配置的冷却时间。DNS、连接中断等暂态错误按单项指数退避，不阻塞已经成功的文件。

```mermaid
flowchart TD
    B[开始批次] --> R[并发请求]
    R --> Q{出现 403/429?}
    Q -- 是 --> DOWN[并发减半]
    DOWN --> COOL[等待 Retry-After / cooldown]
    COOL --> B
    Q -- 否 --> E{暂态错误?}
    E -- 是 --> RETRY[单项指数退避并重排队]
    RETRY --> B
    E -- 否 --> UP[成功窗口后渐增并发]
    UP --> N{仍有任务?}
    N -- 是 --> B
    N -- 否 --> DONE[写入最终状态]
```

## 安全边界

- 不求解验证码、不构造绕过请求、不绕过访问权限。
- 不读取个人浏览器数据库；只操作应用专属 Chrome profile。
- Cookie 不写入日志、manifest、下载状态或失败清单。
- 下载仅接受成功状态和已识别的 `image/*` MIME。
- URL、Cookie 文件、并发值和 Range 响应均在使用前校验。
