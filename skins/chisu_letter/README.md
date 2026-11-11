# 尺素 · 梅影

一款以传统中文信纸为意象的 Paper Shell 主题：暖白宣纸、细浅纸纹、淡墨梅枝，右下角钤一方「尺素」朱印。装饰集中在纸面边缘，正文区域保持留白。

## 字体

主题指定 **思源宋体 / Noto Serif CJK SC Regular**，横竖笔画有清楚的宋体衬线，字面端正而不刻板，适合书信与长文阅读。

- 字体来源：https://github.com/notofonts/noto-cjk/tree/main/Serif
- 字体文件：https://raw.githubusercontent.com/notofonts/noto-cjk/main/Serif/OTF/SimplifiedChinese/NotoSerifCJKsc-Regular.otf
- 许可：SIL Open Font License 1.1（见同仓库 `Serif/LICENSE`）

本次已将字体安装到当前 macOS 用户的 `~/Library/Fonts/NotoSerifCJKsc-Regular.otf`，并保留许可文本。皮肤本身不打包字体；在其他电脑使用时需先安装同名字体，否则应用会回退到系统字体。

## 设计

- **正文**：15.5 px，1.8 倍行距，墨褐色 `#332E28`。
- **纸面**：`#F5F0E4`，四周 28 px 横向留白与 22 px 纵向留白。
- **界面**：米灰侧栏与标题栏，朱砂色 `#8C3B32` 仅作少量强调。
- **背景**：1800 × 1200 PNG，原创程序绘制；包含宣纸纤维、淡墨梅枝与朱印，不使用第三方图片素材。
- **背景强度**：90%。纸纹与墨色已预先淡化，可在预览页或皮肤设置中继续调整。

## 使用

在 **设置 → 皮肤与外观 → 导入皮肤…** 中选择本目录的 `skin.toml`，应用会同时导入 `background.png`。

也可以用生成的 `chisu_letter.paperskin` 一次性导入。打开 `preview.html` 可在浏览器里查看整体效果，并拖动滑块调整背景强度。
