# SMSClient 项目复盘总结

> **日期**: 2026-09-24
> **版本**: v0.1.0
> **状态**: ✅ M0–M6 全部完成，实机验收通过

---

## 1. 里程碑完成情况

| 阶段 | 内容 | 状态 | 交付 |
|------|------|------|------|
| M0 | PRD 确认 + Q7/Q8 拍板 | ✅ 09-22 | docs/PRD.md v1.0 冻结 |
| M1 | Tauri 2 工程初始化 + ASTeam UI | ✅ 09-22 | 7 页面 + 设计令牌 + 4 mock commands |
| M2 | SQLite 数据层 + 联系人 CRUD | ✅ 09-22 | 四表 schema + 消息/设置存取 |
| M3 | WinRT SMS 通道集成 | ✅ 09-23 | 收发短信 + SIM/信号 + 事件订阅 |
| M4 | 收件箱/写短信/已发送 页面联调 | ✅ 09-23 | 会话聚合 + 搜索 + 实时监听 + 联系人选择器 + 发送确认 + 失败重发 |
| M5 | 群发 + 送达回执 + 通知 | ✅ 09-23 | 群发队列(重试/限速/进度) + SmsStatusMessage 回执 + Win11 Toast |
| M6 | 端到端验收 | ✅ 09-24 | 实机启动验证通过（用户确认可用） |

## 2. 技术栈与架构

- **前端**: React 19 + Tailwind CSS 4 + lucide-react + react-router-dom 7
- **后端**: Tauri 2 + rusqlite (bundled SQLite) + windows crate 0.61
- **SMS 通道**: A) WinRT SmsDevice2（当前启用）；B) AT Serial（预留 trait 接口）
- **通知**: tauri-plugin-notification 2.4.0 (Win11 Toast)
- **交付**: 单文件 exe 4.2MB（远低于 PRD 30MB 上限），免安装

## 3. 验收结果 (M6)

- ✅ exe 启动正常，SIM 状态 / 信号 / 通道状态正确显示
- ✅ WinRT 通道就绪（恢复 LTE 适配器后），CMCC -83dBm / 48%
- ✅ 4G 数据连接保持正常（ping 223.5.5.5 0% 丢包）
- ✅ 收发短信、会话视图、群发、设置页面全部可用

## 4. 已知限制 / 后续改进

| 项 | 现状 | 建议 |
|----|------|------|
| ICCID/IMSI | WinRT 读取不到，显示 PRD 占位值 | AT 通道（PID_0312）时可读 |
| 长短信 | 计数/预览已做，物理拆分由运营商处理 | v2.0 加 UDH 拼接控制 |
| MSI 安装包 | WiX 下载超时未生成（exe 不受影响） | 网络畅通时 `tauri build` 重试 |
| 数据位置 | SQLite 在 %APPDATA%\com.asteam.smsclient | v2.0 可加便携模式选项 |
| 送达回执 | TP-ST=0 判定 delivered，匹配最近一条 sent | 加 MessageReferenceNumber 精确匹配 |
| 通知点击跳转 | Toast 点击激活窗口（默认行为） | v2.0 用 onAction 跳转会话 |

## 5. 复盘经验（技术教训）

### WinRT / windows crate
1. `windows::core::Ref<'_, T>` 调方法报 "method not found"：Deref 目标是 `T::Default`，需 `args.as_ref()` 取 `Option<&T>` 再调用
2. WinRT 事件 handler 传参报 `Param` trait 不满足：handler 要传 `&handler`（引用），不是所有权值
3. `SmsStatusMessage` 是送达回执（TP-SRI），`Status()==0` 为送达成功，cast 失败才是普通文本短信
4. 状态事件回调里不能嵌套 `tauri::async_runtime::block_on`（会死锁），推入全局缓冲 + 命令层消费是安全模式

### Tauri 2
5. Tauri 2 插件（notification 等）必须建 `capabilities/default.json` 声明权限（`core:default` + 插件权限），否则运行时静默失败
6. 同步 command 内 `db.with(|conn| ...)` 闭包须返回 `rusqlite::Result`，纯值要包 `Ok(...)`
7. 后台线程访问 SQLite：用 `db::open()` 独立连接 + WAL + busy_timeout，别共享 Mutex<Connection> 跨线程

### SQLite
8. `ORDER BY created_at DESC` 同秒写入顺序不稳定，必须加 `, id DESC` 二级排序（测试暴露）

### Windows 运维
9. Hermes 排障禁用过 LTE 适配器 → WinRT SMS 找不到设备（错误 0x00000000 回退 Mock）。恢复：`Enable-NetAdapter`（Quectel EM05-CN），固件/网络设备自动恢复 OK
10. git-bash (MSYS) 下反斜杠路径 `\r` `\t` 会被转义，启动 exe 用 `powershell.exe Start-Process 'D:\...'` 最稳

## 6. Git 提交历史（M0–M6）

| commit | 说明 |
|--------|------|
| f319d69 | [m5] 群发+送达回执+桌面通知 |
| 2570a2d | [m4] 收件箱/写短信/已发送 页面联调 |
| 0bf657e | [m3] subscribe_inbox_events 编译修复 |
| c096fbc | [m3] get_inbox 自动同步设备短信 |
| 8898b76 | [m3] WinRT 真实通道集成 |
| a342cda | [m3] SmsChannel trait 抽象 |
| 0adc6f7 | [m2] SQLite 数据层 |
| f7583c6 | [m1] 初始脚手架 |

## 7. v2.0 展望（PRD §1.3 排除项）

- 自动回复 / 规则引擎
- AT 串口通道 B（USB PID_0312 切换，完整 PDU 控制）
- 端到端加密
- 便携模式（数据存 exe 同目录）
- 通知点击跳转到对应会话
