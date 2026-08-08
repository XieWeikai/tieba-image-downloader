# 百度贴吧页面结构调研与解析依据

[English](tieba-page-structure.en.md) | 简体中文

## 调研状态

2026-08-09 使用公开帖子 `https://tieba.baidu.com/p/10918721568` 测试。无会话预检返回安全验证；独立 Chrome 完成官方验证并建立会话后，页面为 Vue 客户端渲染，正文数据来自 `POST /c/f/pb/page_pc`。全帖实测 11 页、216 张去重正文原图。

程序检测验证页或 CSR 空壳后启动独立 Chrome，通过 CDP 等待官方页面进入正常帖子状态。它捕获浏览器已经发出的页面 API 响应，不构造或逆向签名，也不自动处理验证码。

## 已实现的结构契约

- 用 URL parser 解析 `http/https`、缺省 scheme 及 `/p/<数字>`；忽略用户的 `pn` 与 `see_lz`。
- 请求固定为 `/p/<id>?pn=N`，只看楼主时追加 `see_lz=1`。
- 总页数读取 `li.l_reply_num span.red`、`a.last.pagination-item`、`.l_pager a` 的文本和 `pn`，取最大正整数。
- 楼层限定 `div.l_post`，正文限定 `.d_post_content`/`.j_d_post_content`，排除正文外头像与装饰。
- 从 `data-field` JSON 读取作者、post ID、楼层，支持 `data-pid` 回退。
- 图片优先级：`data-original`、正文原图链接、`data-src`、`src`；排除表情类。
- 协议相对 URL 转 HTTPS；`/forum/w=580/<name>` 保守转为 `/forum/pic/item/<name>`。
- 下载允许有限重定向，要求成功状态与可识别 `image/*`；HTML、JSON、验证页和空响应不落盘。
- 并发页面结果按 `(页码, 楼层顺序, 图片顺序)` 重排，再按规范 URL 去重并保留首次出现项。
- 新版 API 解析 `first_floor` 与 `post_list[].content`，仅接受 `type=3` 的正文图片；URL 优先级为 `origin_src`、`big_cdn_src`、`cdn_src_active`、`cdn_src`，作者由 `user_list` 映射。

## 后续验证范围

仍需在更多不同类型帖子上持续验证只看楼主模式、折叠/删除楼层、视频混排、超长分页和 CDN Range 差异。
