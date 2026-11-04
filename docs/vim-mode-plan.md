# Vim 模式（Esc 切换，适配中文输入法）实现方案

> 状态：方案，未实现。目标读者：实现者。
> 版本基线：egui/eframe 0.33.3、egui-winit 0.33.3、winit 0.30.12（见 Cargo.lock）。

## 1. 目标体验

- 主编辑器内按 `Esc` → 进入 **普通模式（Normal）**；按 `i / a / o …` → 回到 **插入模式（Insert）**。
- **全程不需要切换系统输入法**。系统输入源一直是「拼音」也没问题：
  - Normal 模式下按 `j` 就是光标下移，不会弹出拼音候选框；
  - 回到 Insert 后第一个键就直接是拼音输入。
- 拼音正在组字（有下划线预编辑文本）时按 `Esc`：先由输入法取消组字（系统惯例），**不会**切模式；再按一次 `Esc` 才进 Normal。不会丢字，也不会把半截拼音留在文档里。
- 默认关闭，在设置里开启（「Vim 模式」开关），关闭时行为与现在完全一致。

## 2. 核心思路：不切输入法，而是在 Normal 模式「关闭 IME」

没有去调 macOS 的 `TISSelectInputSource`（im-select / macism 那套）切换输入源，而是告诉系统「当前没有文本输入框」，这样输入法根本不会介入。

已在依赖源码里核实的链路：

1. egui 每帧的 `PlatformOutput.ime` 是 `Some` 时表示需要 IME。聚焦的 `TextEdit` 会设置它（`egui/src/widgets/text_edit/builder.rs:797`），我们的 `Editor::fix_macos_ime` 也会设置。
2. egui-winit 会根据 `ime.is_some()` 调用 `window.set_ime_allowed(..)`（`egui-winit/src/lib.rs:910-914`，带去抖）。
3. winit macOS 的 `keyDown:`：只有 `ime_allowed == true` 时才调用 `interpretKeyEvents`（交给输入法）；否则直接发出原始的 `KeyboardInput`（`winit/src/platform_impl/macos/view.rs:460-491`）。
   - 输入法处于 Preedit 状态时按键被输入法吃掉，**app 收不到**这个 Esc → 天然实现了「第一次 Esc 取消组字」。

所以：**Normal 模式下，在 `TextEdit::show` 之后把 `o.ime` 设回 `None`，并跳过 `fix_macos_ime`**。下一帧起系统就把按键当原始按键发过来（`Event::Key` + ASCII 的 `Event::Text`，即使输入源是拼音）。切回 Insert 时不再清空 `o.ime`，IME 自动恢复，输入源还是用户原来的拼音。

时序：`Esc` 这一帧处理完 → 帧末 `set_ime_allowed(false)` → 下一个键已是原始按键。按 `i` 那一帧：`i` 本身是原始键（被我们吃掉），帧末 IME 恢复 → 下一个键进拼音。按键本身会触发重绘，不需要额外 `request_repaint`。

## 3. 必须处理的 egui 坑：Esc 会让编辑器失焦

`egui/src/memory/mod.rs:560`：每帧 `begin_pass` 时，如果聚焦控件的 `EventFilter.escape == false`，按 Esc 会直接 `focused_widget = None`。这发生在我们的代码运行之前，事后再 consume 已经来不及。

解决：Vim 模式开启时，在 `TextEdit::show` **之后**覆盖主编辑器的焦点过滤器（下一帧 `begin_pass` 会用它）：

```rust
ui.memory_mut(|m| m.set_focus_lock_filter(id, egui::EventFilter {
    horizontal_arrows: true,
    vertical_arrows: true,
    tab: false,      // 与 TextEdit 默认值保持一致
    escape: true,    // 关键：Esc 不再让编辑器失焦
}));
```

（TextEdit 自己每帧在 show 内部设置一次过滤器，所以必须放在 show 之后。TextEdit 本身不处理 Escape 事件，放行无副作用。）

## 4. 代码结构

新增 `src/ui/vim.rs`（纯逻辑，便于单测），改动集中在 `src/ui/editor.rs::show`：

```rust
pub enum VimMode { Insert, Normal }

pub struct VimState {
    pub enabled: bool,          // 来自设置
    pub mode: VimMode,
    pending: Option<char>,      // 多键命令前缀：g（gg）、d（dd）
}
```

`Editor` 新增字段 `vim: VimState`。`Editor::show` 中的调用顺序（**顺序很重要**）：

1. `let focused = ui.memory(|m| m.has_focus(id));` — 只在主编辑器聚焦时生效，不影响 AI 输入框、搜索框等其他 TextEdit。
2. `self.vim.preprocess_events(ui, focused, &content, cursor)` — 在 `handle_ai_undo` **之前**执行（这样 `u` 转换出的 Cmd+Z 也能触发 AI 撤销）：
   - Insert 模式：只拦截 `Event::Key { key: Escape, pressed: true }` → 切到 Normal 并移除该事件；其他事件不动。
   - Normal 模式：用 `ui.input_mut(|i| i.events = …)` 重写事件队列：
     - 把 Vim 命令翻译成 TextEdit 原生能处理的合成按键（见第 5 节），复用 egui 现成的光标移动/折行/撤销逻辑；
     - 丢弃所有会改文本的事件：`Text`、`Ime(_)`、`Paste`、`Cut`、`Backspace/Delete/Enter/Tab` 等；
     - 放行：方向键、`Cmd+C`、`Cmd+A`、鼠标事件、`Cmd+…` 应用级快捷键（保存、搜索等）。
3. `self.handle_ai_undo(ui)` → `TextEdit::show`（现有逻辑）。
4. show 之后：
   - Vim 开启时调用第 3 节的 `set_focus_lock_filter`；
   - Normal 模式：`ui.ctx().output_mut(|o| o.ime = None)`，不调用 `fix_macos_ime`；
   - 绘制 Normal 模式的块状光标：用 `text_range_screen_rect(&output, i, i+1)`（已存在）取当前字符矩形，画半透明填充（行尾/空行时画一个半角宽的块）。
5. 需要直接改文本/光标的命令（`dd`、`o/O`、`I/A`）：在 show 之前通过 `egui::TextEdit::load_state / store_state` + `state.cursor.set_char_range(..)` 修改光标，或直接改 `content` 字符串后设置光标；改完文本要清 `cached_word_count` 和搜索匹配（与 `editor_response.changed()` 分支一致）。

判断按键：命令字符一律读 `Event::Text`（IME 关闭时是 ASCII，`$`、`G`、`0` 等 shift 组合都天然正确），`Esc` 和 `Ctrl+r` 读 `Event::Key`。

## 5. 首版命令集（MVP）

写作编辑器只需要「快速移动 + 少量编辑」，不追求完整 Vim。

| 按键 | 动作 | 实现方式 |
|---|---|---|
| `Esc` | Insert → Normal（Normal 下：清除选区） | 自定义 |
| `i` / `a` | 光标前 / 后进入 Insert | `a` = 合成 `ArrowRight` + 切模式 |
| `I` / `A` | 行首 / 行尾进入 Insert | 合成 `Home` / `End` + 切模式 |
| `o` / `O` | 下 / 上方新开一行进入 Insert | 直接改 `content` + 设光标 |
| `h j k l` | 左 下 上 右 | 合成方向键（`j/k` 自动按折行后的视觉行移动，适合中文长段落） |
| `w` / `b` | 下 / 上一个词 | 合成 `Alt+ArrowRight/Left`（egui 在 mac 上用 alt，其他平台 ctrl，见 builder.rs:1174） |
| `0` / `$` | 行首 / 行尾 | 合成 `Home` / `End` |
| `gg` / `G` | 文首 / 文末 | 合成 `Cmd+ArrowUp/Down`（mac）；实现时确认 egui 对应的修饰键 |
| `x` | 删除光标处字符 | 合成 `Delete` |
| `dd` | 删除当前行（段落） | 直接改 `content` |
| `u` / `Ctrl+r` | 撤销 / 重做 | 合成 `Cmd+Z` / `Cmd+Shift+Z` |

二期（可选）：数字前缀（`3j`）、`d/c/y + motion`、`v` 可视模式、`p` 粘贴、`/` 打开现有搜索框、中文分词级的 `w/b`。

## 6. UI 与设置

- `src/config.rs::Settings` 加 `vim_mode: bool`（`#[serde(default)]`，默认 `false`，旧配置文件兼容）。
- `src/ui/settings.rs` 加一个复选框「Vim 模式（Esc 进入普通模式）」。
- `src/ui/title_bar.rs` 状态栏现有的字数/时间旁边显示模式：`普通` / `插入`（仅在开启时显示）。
- 普通模式块状光标 + 状态栏标签，两处提示足够，不需要改变背景色。

## 7. 测试

单元测试（`vim.rs`，不依赖窗口）：
- 给定事件序列 + 模式，断言输出事件队列与新模式。例：Normal + `Text("j")` → `[Key(ArrowDown)]`；Normal + `Text("你")`/`Ime(Commit)` → 被丢弃；Insert + `Key(Escape)` → 模式变 Normal 且事件被移除；`g` `g` 跨帧 pending 正确。
- `dd` / `o` / `O` 的文本变换：首行、末行、空文档、含中文多字节字符（用 char 索引，不要用 byte 索引）。

手动测试（macOS，输入源设为简体拼音）：
1. 输入「nihao」出候选 → 按 Esc：候选消失、文档无残留、状态栏仍是「插入」。
2. 再按 Esc：状态栏变「普通」，光标变块状。
3. 连按 `jjkk hl w b gg G`：光标移动，无任何拼音候选框出现。
4. 按 `i`，直接打 `nihao` + 空格：出「你好」，全程没碰输入法切换键。
5. 打开 AI 内联输入框 / 搜索框，在其中按 Esc：行为与现在一致（关闭弹窗），主编辑器模式不受影响。
6. 关闭设置中的 Vim 模式：Esc 行为与现状完全一致。

## 8. 风险与未决

- **Windows / Linux**：`set_ime_allowed(false)` 在 winit 的 Windows（IMM 解关联）和 X11/Wayland 上也有实现，但未实测；首版可以只在 macOS 上承诺该体验。
- **第三方输入法**（搜狗、微信输入法等）：只要它们走标准 NSTextInputClient，关闭 IME 后同样不会介入；需要实测一个第三方输入法。
- **Esc 两次语义**：组字时第一次 Esc 只取消组字，这是有意设计（防丢字）。如果用户希望「一次 Esc 提交拼音并进 Normal」，需要额外处理 `Ime::Preedit` 状态，且不同输入法 Esc 行为不一致，不建议做。
- **`w/b` 对中文**：egui 的词边界会把连续汉字当成一个「词」，`w` 在中文段落里会一次跳到标点/空格。首版接受，二期可按标点或字跳。
- **合成按键依赖 egui 内部行为**（Home/End、Cmd+↑↓、Cmd+Z 的含义）：升级 egui 时需回归第 7 节手动测试。
- `set_focus_lock_filter` 是 egui 的公开 API，但「必须在 TextEdit::show 之后调用」这一点依赖其内部顺序，需写注释说明。
