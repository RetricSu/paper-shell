<div align="center">
  <img src="assets/app-icon.png" alt="Paper Shell app icon" width="96" />
  <h1>Paper Shell</h1>
  <p><strong>A compact writing editor where the document stays first.</strong></p>
  <p>
    Paper Shell is a quiet desktop workspace for everyday and long-form writing.
    It pairs a focused editor with controlled AI assistance, so ideas can be discussed,
    structured, and revised without handing authorship away.
  </p>
  <p>
    <a href="https://github.com/RetricSu/paper-shell/pull/19"><strong>View the AI agent workflow demo</strong></a>
    ·
    <a href="#build">Build locally</a>
    ·
    <a href="#roadmap">Roadmap</a>
  </p>
</div>

> [!TIP]
> The latest demo is the controlled AI agent workflow: tool feedback, document edit proposals, and document-grounded Mermaid mind maps. [Open the demo notes](https://github.com/RetricSu/paper-shell/pull/19).

## Why Paper Shell

- Keep the writing surface primary, with a compact interface that stays out of the way.
- Ask an AI partner to inspect, discuss, and propose changes without mutating the document automatically.
- Review grounded edit proposals before applying them, with stale-snapshot and ambiguous-match protection.
- Turn document context into structured artifacts such as Mermaid mind maps and source-backed outlines.

## Current Focus

Paper Shell is an early desktop app for writers who want a dependable local editor with visible, controlled AI actions. The project is intentionally restrained: fewer distractions, clearer state, and explicit confirmation before anything changes the document.

## Build

```bash
cargo build --release
```

## Roadmap

- [x] fix open-with on Mac and other system
- [x] search and replace
- [x] support copy-paste and right click context menu
- [x] select a text and high-light all the matched ones
- [x] add controlled AI agent workflow
- [ ] support narrative map with click-to-jump function
- [ ] add a test panel that use AI as readers to give feedbacks
- [ ] informative data records on paragraph or full text
- [ ] refine Chinese punctuation handling, likely with a richer text layout engine
- [ ] expand add-ons and export workflows
