# 云端备份安全约定

## 凭据与升级

WebDAV 密码、Bearer Token 和端到端加密主密码只保留在本次进程内，TOML/JSON 序列化均省略；旧版配置中的密码字段不再读入。点击「保存配置」后，连接配置持久化，密码供本次运行使用；重启后必须重新输入。请先确认自己保管了原主密码，再升级或清理旧配置，否则无法解密已有备份。

旧配置文件直到下一次成功保存才移除明文密码。应用不能擦除旧文件副本、文件系统快照或历史备份，应按需自行清理；已暴露的 WebDAV 凭据应轮换。不要在 URL 中嵌入凭据，URL 仅允许 HTTP(S) 目录地址，不接受 userinfo、query 或 fragment。

已调研 [keyring 3.6.3](https://docs.rs/keyring/3.6.3/keyring/) 的 macOS Keychain、Windows Credential Manager 和 Linux Secret Service 后端。本次采用跨平台会话存储，未接入系统凭据库，也没有把加密密钥与密文一起存盘。后续接入应采用独立的服务/账户命名空间、明确的用户授权及锁定/不可用错误处理；不允许失败后退回明文。Linux 应验证持久化 Secret Service 的可用性，不能把 mock 或会话 keyutils 当作持久存储。

## 传输与元数据

非 localhost / 字面 loopback IP 的 HTTP 地址会在配置窗口显示风险提示，局域网地址同样提示。HTTP 仍可使用，端到端加密只保护正文，不能保护 Basic/Bearer 登录凭据。建议始终使用 HTTPS。HTTP agent 禁止自动重定向，避免请求从 HTTPS 降级或写入其他目标；请直接填写最终 WebDAV 地址。

新上传的 payload 始终省略 source_path，包括启用端到端加密时。旧备份仍可读取该字段，但恢复不据此选择本地写入路径。文件名、时间、字数和正文在未加密备份中仍为明文，doc_id 与远程文件名在加密备份中仍可见。doc_id 保留既有路径哈希以维持备份定位兼容，它不是匿名化标识，已知候选路径仍可被猜测验证。需要隐藏文件名时可将远程模板设为 `{doc_id}.backup`。

## 密码学与内存

新加密备份使用 v2 信封，AES-256-GCM、16 字节随机盐、12 字节随机 nonce，PBKDF2-HMAC-SHA256 固定 600,000 轮。v1 加密备份仍按原 100,000 轮读取；未加密备份保持 v1。旧客户端不能读取 v2 加密备份。采用固定版本参数，拒绝未知版本和非法盐/nonce 长度，避免远端任意指定 KDF 成本。

600,000 轮参考 [OWASP PBKDF2 建议](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html#pbkdf2)。Argon2id 的内存硬度更适合抵抗 GPU 猜测，但需要另一个版本的格式、平台内存预算和迁移评估，本次未切换算法。加密仍在线程中执行。

密码字段及其工作线程副本、派生密钥、序列化加密明文、解密缓冲和 HTTP 正文缓冲由 `Zeroizing` 在释放时擦除，启用 aes-gcm 的 zeroize 功能清理 AES/GHASH 内部状态。配置 Debug 输出脱敏。参见 [zeroize 的保证与限制](https://docs.rs/zeroize/latest/zeroize/)：它不能保证擦除重分配前的副本、寄存器、GUI 编辑/撤销缓存、HTTP 头副本、交换区或崩溃转储；这是尽力防护，并非内存隔离。返回给编辑器的文档正文需继续使用，不自动清除。

## 覆盖与竞态

上传先 GET 检查文档身份。仅 404 视为不存在，随后 PUT 带 `If-None-Match: *`；已存在且文档身份一致时必须有强 ETag，PUT 带 `If-Match`。权限错误、重定向、网络错误、无法识别的数据、其他文档以及缺失/弱 ETag 均停止覆盖。412 作为覆盖冲突报告。

防竞态依赖服务端遵守 HTTP 条件请求。忽略条件头的非标准服务器仍可能产生覆盖竞态；纯客户端无法保证这种服务器上的原子性。无强 ETag 的服务器需要更换远程文件名或升级服务端，不能通过无条件 PUT 回退绕过保护。
