//! Document export functionality supporting PDF, HTML, Markdown, and Plain Text.

use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Pdf,
    Html,
    Markdown,
    PlainText,
}

impl ExportFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Pdf => "pdf",
            ExportFormat::Html => "html",
            ExportFormat::Markdown => "md",
            ExportFormat::PlainText => "txt",
        }
    }

    pub fn filter_name(&self) -> &'static str {
        match self {
            ExportFormat::Pdf => "PDF 文档 (*.pdf)",
            ExportFormat::Html => "HTML 文档 (*.html)",
            ExportFormat::Markdown => "Markdown 文档 (*.md)",
            ExportFormat::PlainText => "纯文本文档 (*.txt)",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ExportFormat::Pdf => "PDF 文档",
            ExportFormat::Html => "HTML 文档",
            ExportFormat::Markdown => "Markdown",
            ExportFormat::PlainText => "纯文本",
        }
    }

    pub fn menu_label(&self) -> &'static str {
        match self {
            ExportFormat::Pdf => "PDF 文档 (.pdf)…",
            ExportFormat::Html => "HTML 文档 (.html)…",
            ExportFormat::Markdown => "Markdown (.md)…",
            ExportFormat::PlainText => "纯文本 (.txt)…",
        }
    }
}

/// Generates a suggested export filename based on the current document path and format.
pub fn suggested_file_name(current_file: Option<&Path>, format: ExportFormat) -> String {
    let stem = current_file
        .and_then(|p| p.file_stem())
        .and_then(|s| s.to_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("未命名");
    format!("{}.{}", stem, format.extension())
}

/// Exports the document content to the specified path in the given format.
pub fn export_document(
    format: ExportFormat,
    content: &str,
    path: &Path,
    title: &str,
) -> Result<(), String> {
    if content.trim().is_empty() {
        return Err("当前文档为空，无法导出".to_string());
    }

    match format {
        ExportFormat::Pdf => export_pdf(content, path, title),
        ExportFormat::Html => export_html(content, path, title),
        ExportFormat::Markdown => export_text_file(content, path),
        ExportFormat::PlainText => export_text_file(content, path),
    }
}

/// Writes plain text or markdown directly to disk.
pub fn export_text_file(content: &str, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("无法创建目标目录 {}：{}", parent.display(), e))?;
    }
    fs::write(path, content).map_err(|e| format!("写入文件失败 {}：{}", path.display(), e))
}

/// Exports the document to a standalone styled HTML file.
pub fn export_html(content: &str, path: &Path, title: &str) -> Result<(), String> {
    let html = render_html(content, title);
    export_text_file(&html, path)
}

/// Exports the document directly to a PDF file.
#[cfg(target_os = "macos")]
pub fn export_pdf(content: &str, path: &Path, title: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("无法创建目标目录 {}：{}", parent.display(), e))?;
    }
    crate::plugin::builtin::print::macos::export_pdf_to_path(content, path, title)
}

/// Fallback for non-macOS platforms.
#[cfg(not(target_os = "macos"))]
pub fn export_pdf(_content: &str, _path: &Path, _title: &str) -> Result<(), String> {
    Err(
        "当前平台不支持原生 PDF 导出。建议导出为 HTML 文档并在浏览器中打印或另存为 PDF。"
            .to_string(),
    )
}

/// Escapes HTML special characters in plain text.
fn escape_html(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(c),
        }
    }
    escaped
}

/// Parses inline markdown elements (code, bold, italic, links, images, strikethrough).
fn parse_inline_markdown(input: &str) -> String {
    let mut result = String::with_capacity(input.len() + 32);
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Inline code `...`
        if chars[i] == '`'
            && let Some(end) = chars[i + 1..].iter().position(|&c| c == '`')
        {
            let code_content: String = chars[i + 1..i + 1 + end].iter().collect();
            result.push_str("<code>");
            result.push_str(&escape_html(&code_content));
            result.push_str("</code>");
            i += end + 2;
            continue;
        }

        // Image ![alt](url)
        if chars[i] == '!'
            && i + 1 < len
            && chars[i + 1] == '['
            && let Some(alt_end) = chars[i + 2..].iter().position(|&c| c == ']')
        {
            let alt_idx = i + 2 + alt_end;
            if alt_idx + 1 < len
                && chars[alt_idx + 1] == '('
                && let Some(url_end) = chars[alt_idx + 2..].iter().position(|&c| c == ')')
            {
                let alt: String = chars[i + 2..alt_idx].iter().collect();
                let url: String = chars[alt_idx + 2..alt_idx + 2 + url_end].iter().collect();
                result.push_str(&format!(
                    "<img src=\"{}\" alt=\"{}\" />",
                    escape_html(&url),
                    escape_html(&alt)
                ));
                i = alt_idx + 3 + url_end;
                continue;
            }
        }

        // Link [text](url)
        if chars[i] == '['
            && let Some(text_end) = chars[i + 1..].iter().position(|&c| c == ']')
        {
            let text_idx = i + 1 + text_end;
            if text_idx + 1 < len
                && chars[text_idx + 1] == '('
                && let Some(url_end) = chars[text_idx + 2..].iter().position(|&c| c == ')')
            {
                let text: String = chars[i + 1..text_idx].iter().collect();
                let url: String = chars[text_idx + 2..text_idx + 2 + url_end].iter().collect();
                result.push_str(&format!(
                    "<a href=\"{}\">{}</a>",
                    escape_html(&url),
                    parse_inline_markdown(&text)
                ));
                i = text_idx + 3 + url_end;
                continue;
            }
        }

        // Strikethrough ~~text~~
        if chars[i] == '~'
            && i + 1 < len
            && chars[i + 1] == '~'
            && let Some(end) = (i + 2..len - 1).find(|&j| chars[j] == '~' && chars[j + 1] == '~')
        {
            let inner: String = chars[i + 2..end].iter().collect();
            result.push_str("<del>");
            result.push_str(&parse_inline_markdown(&inner));
            result.push_str("</del>");
            i = end + 2;
            continue;
        }

        // Bold **text** or __text__
        if (chars[i] == '*' && i + 1 < len && chars[i + 1] == '*')
            || (chars[i] == '_' && i + 1 < len && chars[i + 1] == '_')
        {
            let marker = chars[i];
            if let Some(end) =
                (i + 2..len - 1).find(|&j| chars[j] == marker && chars[j + 1] == marker)
            {
                let inner: String = chars[i + 2..end].iter().collect();
                result.push_str("<strong>");
                result.push_str(&parse_inline_markdown(&inner));
                result.push_str("</strong>");
                i = end + 2;
                continue;
            }
        }

        // Italic *text* or _text_
        if chars[i] == '*' || chars[i] == '_' {
            let marker = chars[i];
            if let Some(end) = chars[i + 1..].iter().position(|&c| c == marker) {
                let inner: String = chars[i + 1..i + 1 + end].iter().collect();
                if !inner.is_empty() && !inner.contains('\n') {
                    result.push_str("<em>");
                    result.push_str(&parse_inline_markdown(&inner));
                    result.push_str("</em>");
                    i += end + 2;
                    continue;
                }
            }
        }

        // Default: escape char
        match chars[i] {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#39;"),
            c => result.push(c),
        }
        i += 1;
    }

    result
}

/// Converts markdown body content into an HTML fragment.
pub fn markdown_to_html_body(content: &str) -> String {
    let mut output = String::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    let len = lines.len();

    let mut in_ul = false;
    let mut in_ol = false;
    let mut in_blockquote = false;

    let close_blocks =
        |output: &mut String, in_ul: &mut bool, in_ol: &mut bool, in_bq: &mut bool| {
            if *in_ul {
                output.push_str("</ul>\n");
                *in_ul = false;
            }
            if *in_ol {
                output.push_str("</ol>\n");
                *in_ol = false;
            }
            if *in_bq {
                output.push_str("</blockquote>\n");
                *in_bq = false;
            }
        };

    while i < len {
        let line = lines[i];
        let trimmed = line.trim();

        // Fenced code block ```[lang]
        if trimmed.starts_with("```") {
            close_blocks(&mut output, &mut in_ul, &mut in_ol, &mut in_blockquote);
            let lang = trimmed.trim_start_matches('`').trim();
            output.push_str("<pre><code");
            if !lang.is_empty() {
                output.push_str(&format!(" class=\"language-{}\"", escape_html(lang)));
            }
            output.push('>');
            i += 1;
            let mut code_body = String::new();
            while i < len && !lines[i].trim().starts_with("```") {
                code_body.push_str(lines[i]);
                code_body.push('\n');
                i += 1;
            }
            output.push_str(&escape_html(&code_body));
            output.push_str("</code></pre>\n");
            i += 1;
            continue;
        }

        // Empty line
        if trimmed.is_empty() {
            close_blocks(&mut output, &mut in_ul, &mut in_ol, &mut in_blockquote);
            i += 1;
            continue;
        }

        // Horizontal rule
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            close_blocks(&mut output, &mut in_ul, &mut in_ol, &mut in_blockquote);
            output.push_str("<hr />\n");
            i += 1;
            continue;
        }

        // Headings
        if let Some(heading) = trimmed.strip_prefix('#') {
            close_blocks(&mut output, &mut in_ul, &mut in_ol, &mut in_blockquote);
            let mut level = 1;
            let mut rest = heading;
            while let Some(next) = rest.strip_prefix('#') {
                level += 1;
                rest = next;
                if level == 6 {
                    break;
                }
            }
            if rest.starts_with(' ') {
                let text = rest.trim();
                output.push_str(&format!(
                    "<h{level}>{}</h{level}>\n",
                    parse_inline_markdown(text)
                ));
                i += 1;
                continue;
            }
        }

        // Blockquote
        if let Some(quote) = trimmed.strip_prefix('>') {
            if in_ul {
                output.push_str("</ul>\n");
                in_ul = false;
            }
            if in_ol {
                output.push_str("</ol>\n");
                in_ol = false;
            }
            if !in_blockquote {
                output.push_str("<blockquote>\n");
                in_blockquote = true;
            }
            let text = quote.trim_start();
            output.push_str(&format!("<p>{}</p>\n", parse_inline_markdown(text)));
            i += 1;
            continue;
        } else if in_blockquote {
            output.push_str("</blockquote>\n");
            in_blockquote = false;
        }

        // Unordered list: - , * , +
        let is_ul_item =
            (trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ "))
                && trimmed.len() >= 2;
        if is_ul_item {
            if in_ol {
                output.push_str("</ol>\n");
                in_ol = false;
            }
            if !in_ul {
                output.push_str("<ul>\n");
                in_ul = true;
            }
            let item_text = &trimmed[2..].trim();
            output.push_str(&format!("<li>{}</li>\n", parse_inline_markdown(item_text)));
            i += 1;
            continue;
        }

        // Ordered list: 1. , 2. , etc.
        let is_ol_item = trimmed
            .find('.')
            .map(|pos| {
                pos > 0
                    && trimmed[..pos].chars().all(|c| c.is_ascii_digit())
                    && trimmed[pos + 1..].starts_with(' ')
            })
            .unwrap_or(false);

        if is_ol_item {
            if in_ul {
                output.push_str("</ul>\n");
                in_ul = false;
            }
            if !in_ol {
                output.push_str("<ol>\n");
                in_ol = true;
            }
            let dot_pos = trimmed.find('.').unwrap();
            let item_text = trimmed[dot_pos + 1..].trim();
            output.push_str(&format!("<li>{}</li>\n", parse_inline_markdown(item_text)));
            i += 1;
            continue;
        }

        // Regular paragraph: collect consecutive non-empty non-special lines
        close_blocks(&mut output, &mut in_ul, &mut in_ol, &mut in_blockquote);
        let mut para_lines = Vec::new();
        while i < len {
            let cur = lines[i].trim();
            if cur.is_empty()
                || cur.starts_with('#')
                || cur.starts_with("```")
                || cur.starts_with('>')
                || cur == "---"
                || cur == "***"
                || cur == "___"
                || cur.starts_with("- ")
                || cur.starts_with("* ")
                || cur.starts_with("+ ")
                || (cur
                    .find('.')
                    .map(|p| {
                        p > 0
                            && cur[..p].chars().all(|c| c.is_ascii_digit())
                            && cur[p + 1..].starts_with(' ')
                    })
                    .unwrap_or(false))
            {
                break;
            }
            para_lines.push(parse_inline_markdown(lines[i].trim()));
            i += 1;
        }
        if !para_lines.is_empty() {
            output.push_str(&format!("<p>{}</p>\n", para_lines.join("<br />\n")));
        }
    }

    close_blocks(&mut output, &mut in_ul, &mut in_ol, &mut in_blockquote);
    output
}

/// Renders the document content as a complete, styled HTML page.
pub fn render_html(content: &str, title: &str) -> String {
    let escaped_title = escape_html(title);
    let body_html = markdown_to_html_body(content);

    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>{title}</title>
    <style>
        :root {{
            --bg-color: #faf9f6;
            --text-color: #2b2b2b;
            --heading-color: #1a1a1a;
            --accent-color: #3b5998;
            --code-bg: #f0eee9;
            --border-color: #e5e3dc;
            --quote-border: #99958d;
        }}

        @media (prefers-color-scheme: dark) {{
            :root {{
                --bg-color: #1c1c1e;
                --text-color: #d1d1d6;
                --heading-color: #f2f2f7;
                --accent-color: #64d2ff;
                --code-bg: #2c2c2e;
                --border-color: #38383a;
                --quote-border: #636366;
            }}
        }}

        body {{
            background-color: var(--bg-color);
            color: var(--text-color);
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", sans-serif;
            font-size: 16px;
            line-height: 1.75;
            max-width: 740px;
            margin: 40px auto;
            padding: 0 24px 80px;
            word-wrap: break-word;
        }}

        h1, h2, h3, h4, h5, h6 {{
            color: var(--heading-color);
            line-height: 1.35;
            margin-top: 1.8em;
            margin-bottom: 0.6em;
            font-weight: 600;
        }}

        h1 {{ font-size: 2em; border-bottom: 1px solid var(--border-color); padding-bottom: 0.3em; }}
        h2 {{ font-size: 1.5em; border-bottom: 1px solid var(--border-color); padding-bottom: 0.3em; }}
        h3 {{ font-size: 1.25em; }}

        p {{
            margin: 1.1em 0;
        }}

        a {{
            color: var(--accent-color);
            text-decoration: none;
        }}

        a:hover {{
            text-decoration: underline;
        }}

        code {{
            font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
            font-size: 0.9em;
            background-color: var(--code-bg);
            padding: 0.2em 0.4em;
            border-radius: 4px;
        }}

        pre {{
            background-color: var(--code-bg);
            border-radius: 6px;
            padding: 14px 18px;
            overflow-x: auto;
            line-height: 1.5;
            margin: 1.4em 0;
        }}

        pre code {{
            background: none;
            padding: 0;
            font-size: 0.88em;
        }}

        blockquote {{
            margin: 1.4em 0;
            padding: 0.2em 1.2em;
            border-left: 4px solid var(--quote-border);
            color: var(--text-color);
            opacity: 0.88;
        }}

        ul, ol {{
            padding-left: 28px;
            margin: 1.1em 0;
        }}

        li {{
            margin: 0.4em 0;
        }}

        hr {{
            border: none;
            border-top: 1px solid var(--border-color);
            margin: 2.5em 0;
        }}

        img {{
            max-width: 100%;
            height: auto;
            border-radius: 4px;
        }}

        @media print {{
            body {{
                max-width: 100%;
                margin: 0;
                padding: 16mm;
                background-color: #fff !important;
                color: #000 !important;
            }}
            pre, blockquote {{
                page-break-inside: avoid;
            }}
        }}
    </style>
</head>
<body>
    {body_html}
</body>
</html>
"#,
        title = escaped_title,
        body_html = body_html.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggested_file_name_handles_various_extensions() {
        assert_eq!(
            suggested_file_name(Some(Path::new("/docs/my_notes.txt")), ExportFormat::Pdf),
            "my_notes.pdf"
        );
        assert_eq!(
            suggested_file_name(Some(Path::new("report.markdown")), ExportFormat::Html),
            "report.html"
        );
        assert_eq!(
            suggested_file_name(None, ExportFormat::Markdown),
            "未命名.md"
        );
        assert_eq!(
            suggested_file_name(Some(Path::new("")), ExportFormat::PlainText),
            "未命名.txt"
        );
    }

    #[test]
    fn export_document_rejects_empty_content() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("empty.txt");
        let result = export_document(ExportFormat::PlainText, "   \n\t ", &target, "Test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("为空"));
    }

    #[test]
    fn export_markdown_and_plain_text_writes_exact_content() {
        let temp = tempfile::tempdir().unwrap();
        let md_path = temp.path().join("sub/test.md");
        let content = "# Title\n\nHello, world!";

        export_document(ExportFormat::Markdown, content, &md_path, "Test").unwrap();
        assert_eq!(fs::read_to_string(&md_path).unwrap(), content);

        let txt_path = temp.path().join("test.txt");
        export_document(ExportFormat::PlainText, content, &txt_path, "Test").unwrap();
        assert_eq!(fs::read_to_string(&txt_path).unwrap(), content);
    }

    #[test]
    fn render_html_produces_valid_structure_and_escapes() {
        let content = "# 标题 <测试>\n\n这是**粗体**与*斜体*，还有`代码 & 符号`。\n\n- 列表项 1\n- 列表项 2\n\n> 引用块\n\n```rust\nfn main() {\n    println!(\"<hello>\");\n}\n```";
        let html = render_html(content, "测试文档 & 样例");

        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<title>测试文档 &amp; 样例</title>"));
        assert!(html.contains("<h1>标题 &lt;测试&gt;</h1>"));
        assert!(html.contains("<strong>粗体</strong>"));
        assert!(html.contains("<em>斜体</em>"));
        assert!(html.contains("<code>代码 &amp; 符号</code>"));
        assert!(html.contains("<ul>\n<li>列表项 1</li>\n<li>列表项 2</li>\n</ul>"));
        assert!(html.contains("<blockquote>\n<p>引用块</p>\n</blockquote>"));
        assert!(html.contains("<pre><code class=\"language-rust\">"));
        assert!(html.contains("&lt;hello&gt;"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn export_pdf_requires_main_thread_or_succeeds_on_main() {
        let temp = tempfile::tempdir().unwrap();
        let pdf_path = temp.path().join("document.pdf");
        let content = "这是用于测试 PDF 导出的文本内容。\nEnglish sentence for layout testing.\n多行段落内容测试。";

        let result = export_pdf(content, &pdf_path, "测试文档");
        // In the standard cargo test runner, unit tests run on worker threads,
        // where AppKit strictly enforces main thread execution.
        // Full PDF generation on the main thread is exercised in tests/print_layout.rs.
        if objc2::MainThreadMarker::new().is_none() {
            assert_eq!(result, Err("PDF 渲染必须在主线程执行".to_string()));
        } else {
            assert!(
                result.is_ok(),
                "PDF export should succeed on main thread: {:?}",
                result
            );
            let bytes = fs::read(&pdf_path).unwrap();
            assert!(
                bytes.starts_with(b"%PDF-"),
                "Generated file should have valid PDF magic header"
            );
            assert!(bytes.len() > 100, "PDF file should contain data");
        }
    }
}
