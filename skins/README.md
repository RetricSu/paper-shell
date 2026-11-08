# Paper Shell 皮肤插件仓库 (Skins Repository)

本目录为 Paper Shell 官方皮肤插件仓库。应用内置的皮肤市场默认拉取本仓库的 `skins/` 目录：
- GitHub 仓库: `RetricSu/paper-shell`
- 分支: `master`
- 目录: `skins`

## 目录结构

每个皮肤以独立子目录存在，目录内必须包含 `skin.toml` 描述文件：

```
skins/
├── cyberpunk_neon/
│   └── skin.toml
├── nordic_frost/
│   └── skin.toml
├── retro_amber/
│   └── skin.toml
├── bamboo_grove/
│   └── skin.toml
...
```

欢迎向此目录提交新的皮肤风格！

## 图片背景示例：夏日站台

`summer_platform/` 使用 1993 年《听到涛声》的官方剧照，包含 JPG 背景、配色和排版。
在应用中导入该目录的 `skin.toml` 即可试用，详见 [素材来源和使用说明](summer_platform/README.md)。
升级会将最早默认的 `retricsu/paper-shell-themes · main · skins` 迁移到上述官方源，其他自定义源保留。
